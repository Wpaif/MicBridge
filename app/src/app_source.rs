//! Fonte extra: manda o som de um app (ex.: uma aba do navegador) para o microfone virtual,
//! somado à voz do celular. O app continua tocando no fone normalmente.
//!
//! ```text
//! aba ──► fone (como antes)
//!   └──pw-link──► [micbridge_extra] ──volume──► [vdo_input] ──► MicBridge ──► call
//! ```
//!
//! O `pw-loopback` intermediário só existe com uma fonte escolhida. Nenhum lado dele conecta
//! sozinho (`node.autoconnect=false`): este módulo faz as duas ligações com `pw-link`, então ele
//! não vira uma saída do sistema nem cai no fone se o microfone virtual sumir.

use crate::output::DieWithParent;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::process::{Child, Command};

const NODE: &str = "micbridge_extra";
const NODE_OUT: &str = "micbridge_extra_out";

/// Um app tocando áudio agora (um stream de saída no PipeWire).
#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    pub id: u32,
    /// Ex.: "Firefox (2)".
    pub app: String,
    /// Ex.: o título da aba.
    pub title: String,
    /// App de chamada (Discord etc.): ligar devolveria a call para ela mesma.
    pub call: bool,
    /// Identidade que sobrevive quando o app recria o stream (pausa, troca de vídeo).
    key: (Option<u32>, String),
}

impl Stream {
    pub fn label(&self) -> String {
        if self.title.is_empty() || self.title == self.app {
            self.app.clone()
        } else {
            format!("{} · {}", self.app, self.title)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub streams: Vec<Stream>,
    /// Stream escolhido (o id atual, se o app o recriou).
    pub selected: Option<u32>,
    /// O áudio está de fato chegando ao microfone virtual.
    pub linked: bool,
    /// 0 a 100.
    pub volume: f32,
    pub error: Option<String>,
}

struct State {
    snapshot: Snapshot,
    /// O que o usuário escolheu, para reencontrar o stream se o id mudar.
    wanted: Option<Stream>,
}

#[derive(Clone)]
pub struct AppSource {
    state: Arc<Mutex<State>>,
}

impl AppSource {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                snapshot: Snapshot {
                    volume: 60.0,
                    ..Default::default()
                },
                wanted: None,
            })),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().snapshot.clone()
    }

    /// `None` desliga a fonte extra.
    pub fn select(&self, id: Option<u32>) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        let wanted = match id {
            None => None,
            Some(id) => Some(
                st.snapshot
                    .streams
                    .iter()
                    .find(|s| s.id == id)
                    .cloned()
                    .ok_or("App não encontrado; a lista mudou")?,
            ),
        };
        st.snapshot.selected = wanted.as_ref().map(|s| s.id);
        st.snapshot.linked = false;
        st.snapshot.error = None;
        st.wanted = wanted;
        Ok(())
    }

    pub fn set_volume(&self, percent: f32) {
        self.state.lock().unwrap().snapshot.volume = percent.clamp(0.0, 100.0);
    }
}

/// Dados lidos do `pw-dump` que interessam aqui.
#[derive(Default)]
struct Graph {
    streams: Vec<Stream>,
    /// (id do node, canal, é saída?) -> id da porta
    ports: Vec<(u32, String, bool, u32)>,
    /// (porta de saída, porta de entrada)
    links: Vec<(u32, u32)>,
    /// id do node pelo `node.name` (só os nossos).
    loop_in: Option<u32>,
    loop_out: Option<u32>,
    /// O sink do microfone virtual (`vdo_input`).
    sink: Option<u32>,
}

impl Graph {
    fn ports(&self, node: u32, output: bool) -> Vec<(&str, u32)> {
        self.ports
            .iter()
            .filter(|p| p.0 == node && p.2 == output)
            .map(|p| (p.1.as_str(), p.3))
            .collect()
    }
}

/// O processo `pid` foi aberto por um MicBridge? (o `pw-cat` da voz e o do retorno no fone,
/// deste app ou de outra instância rodando). Mandar esses para o microfone faria eco.
fn is_micbridge_child(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // O nome do processo fica entre parênteses e pode ter espaços: pula até o ")".
    let Some(ppid) = stat
        .rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().nth(1))
        .and_then(|p| p.parse::<u32>().ok())
    else {
        return false;
    };
    ppid == std::process::id()
        || std::fs::read_to_string(format!("/proc/{ppid}/comm"))
            .is_ok_and(|c| c.trim() == "micbridge")
}

/// Lê um número das props, venha ele como número ou como texto.
fn prop_u32(props: &Value, key: &str) -> Option<u32> {
    props[key]
        .as_u64()
        .map(|n| n as u32)
        .or_else(|| props[key].as_str()?.parse().ok())
}

fn parse(dump: &Value) -> Graph {
    let mut g = Graph::default();
    // Nem todo node traz o pid (o `pw-cat` não traz); o client dono dele sempre traz.
    let client_pid: std::collections::HashMap<u32, u32> = dump
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["type"].as_str() == Some("PipeWire:Interface:Client"))
        .filter_map(|o| {
            let id = o["id"].as_u64()? as u32;
            Some((id, prop_u32(&o["info"]["props"], "application.process.id")?))
        })
        .collect();
    for obj in dump.as_array().into_iter().flatten() {
        let Some(id) = obj["id"].as_u64().map(|i| i as u32) else {
            continue;
        };
        let info = &obj["info"];
        let props = &info["props"];
        let text = |k: &str| props[k].as_str().unwrap_or_default().to_owned();
        match obj["type"].as_str().unwrap_or_default() {
            "PipeWire:Interface:Node" => {
                match text("node.name").as_str() {
                    NODE => g.loop_in = Some(id),
                    NODE_OUT => g.loop_out = Some(id),
                    crate::output::SINK => g.sink = Some(id),
                    _ => {}
                }
                if text("media.class") != "Stream/Output/Audio" {
                    continue;
                }
                let pid = prop_u32(props, "application.process.id")
                    .or_else(|| client_pid.get(&prop_u32(props, "client.id")?).copied());
                if text("node.name") == NODE_OUT || pid.is_some_and(is_micbridge_child) {
                    continue;
                }
                let app = [
                    text("application.name"),
                    text("node.description"),
                    text("node.name"),
                ]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or_else(|| format!("App {id}"));
                let binary = text("application.process.binary").to_lowercase();
                let call = [
                    "discord", "vesktop", "webcord", "zoom", "teams", "skype", "slack",
                ]
                .iter()
                .any(|c| binary.contains(c) || app.to_lowercase().contains(c));
                let title = text("media.name");
                g.streams.push(Stream {
                    id,
                    key: (pid, app.clone()),
                    app,
                    title,
                    call,
                });
            }
            "PipeWire:Interface:Port" => {
                if props["port.monitor"].as_bool() == Some(true) || text("port.monitor") == "true" {
                    continue;
                }
                let Some(node) = prop_u32(props, "node.id") else {
                    continue;
                };
                let output = info["direction"].as_str() == Some("output");
                let channel = text("audio.channel");
                g.ports.push((node, channel, output, id));
            }
            "PipeWire:Interface:Link" => {
                if let (Some(o), Some(i)) = (
                    info["output-port-id"].as_u64(),
                    info["input-port-id"].as_u64(),
                ) {
                    g.links.push((o as u32, i as u32));
                }
            }
            _ => {}
        }
    }
    g
}

async fn run(cmd: &str, args: &[&str]) -> Option<Vec<u8>> {
    let out = tokio::time::timeout(
        Duration::from_secs(2),
        Command::new(cmd).args(args).kill_on_drop(true).output(),
    )
    .await;
    match out {
        Ok(Ok(o)) if o.status.success() => Some(o.stdout),
        _ => None,
    }
}

async fn read_graph() -> Option<Graph> {
    let bytes = run("pw-dump", &[]).await?;
    serde_json::from_slice(&bytes).ok().map(|v| parse(&v))
}

fn spawn_loopback() -> anyhow::Result<Child> {
    use anyhow::Context;
    Command::new("pw-loopback")
        .args([
            "-m",
            "[FL FR]",
            &format!(
                "--capture-props=node.name={NODE} node.description=MicBridge_Extra \
                 node.autoconnect=false media.class=Stream/Input/Audio"
            ),
            &format!(
                "--playback-props=node.name={NODE_OUT} node.description=MicBridge_Extra \
                 node.autoconnect=false"
            ),
        ])
        .kill_on_drop(true)
        .die_with_parent()
        .spawn()
        .context("falha ao executar pw-loopback")
}

/// Pares (porta de saída de `from`, porta de entrada de `to`) por canal.
/// Mono vai para os dois lados.
fn wanted_links(g: &Graph, from: u32, to: u32) -> Vec<(u32, u32)> {
    let outs = g.ports(from, true);
    let ins = g.ports(to, false);
    let mut pairs = Vec::new();
    for (ch, inp) in &ins {
        let src = outs
            .iter()
            .find(|(c, _)| c == ch)
            .or_else(|| outs.iter().find(|(c, _)| *c == "MONO"))
            .or_else(|| (outs.len() == 1).then(|| &outs[0]));
        if let Some((_, out)) = src {
            pairs.push((*out, *inp));
        }
    }
    pairs
}

/// Mantém a fonte extra ligada: acompanha a lista de apps, reencontra o stream escolhido
/// quando o app o recria e refaz as ligações que sumirem.
pub async fn supervise(source: AppSource) {
    let mut loopback: Option<Child> = None;
    let mut applied_volume: Option<(u32, f32)> = None;
    loop {
        tick(&source, &mut loopback, &mut applied_volume).await;
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
}

async fn tick(
    source: &AppSource,
    loopback: &mut Option<Child>,
    applied_volume: &mut Option<(u32, f32)>,
) {
    let Some(g) = read_graph().await else {
        source.state.lock().unwrap().snapshot.error = Some("PipeWire indisponível".into());
        return;
    };

    // Atualiza a lista e reencontra a escolha (mesmo id, ou mesmo app/processo).
    let (wanted, volume) = {
        let mut st = source.state.lock().unwrap();
        let found = st.wanted.as_ref().and_then(|w| {
            g.streams
                .iter()
                .find(|s| s.id == w.id)
                .or_else(|| g.streams.iter().find(|s| s.key == w.key))
                .cloned()
        });
        st.snapshot.streams = g.streams.clone();
        if st.wanted.is_some() {
            st.snapshot.selected = found.as_ref().map(|s| s.id);
            if let Some(f) = &found {
                st.wanted = Some(f.clone());
            }
        }
        (
            st.wanted.is_some().then_some(found).flatten(),
            st.snapshot.volume,
        )
    };
    let enabled = source.state.lock().unwrap().wanted.is_some();

    if !enabled {
        if loopback.take().is_some() {
            tracing::info!("fonte extra desligada");
        }
        *applied_volume = None;
        let mut st = source.state.lock().unwrap();
        st.snapshot.linked = false;
        st.snapshot.selected = None;
        return;
    }

    // Loopback vivo? Se morreu ou o node sumiu, recria e espera o próximo ciclo.
    let alive = loopback
        .as_mut()
        .is_some_and(|c| matches!(c.try_wait(), Ok(None)));
    let Some(sink) = g.sink else {
        let mut st = source.state.lock().unwrap();
        st.snapshot.linked = false;
        st.snapshot.error = Some("Microfone virtual ausente; aguardando".into());
        return;
    };
    let (Some(loop_in), Some(loop_out), true) = (g.loop_in, g.loop_out, alive) else {
        if !alive {
            drop(loopback.take());
            *applied_volume = None;
            match spawn_loopback() {
                Ok(c) => *loopback = Some(c),
                Err(e) => source.state.lock().unwrap().snapshot.error = Some(format!("{e:#}")),
            }
        }
        source.state.lock().unwrap().snapshot.linked = false;
        return;
    };

    if *applied_volume != Some((loop_out, volume)) {
        // Fração do slider; o `wpctl set-volume` já aplica a curva cúbica (como o pavucontrol).
        let v = format!("{:.3}", volume / 100.0);
        if run("wpctl", &["set-volume", &loop_out.to_string(), &v])
            .await
            .is_some()
        {
            *applied_volume = Some((loop_out, volume));
        }
    }

    // Desfaz ligações de streams que não são mais o escolhido.
    let keep: Vec<(u32, u32)> = wanted
        .as_ref()
        .map(|s| wanted_links(&g, s.id, loop_in))
        .unwrap_or_default();
    let in_ports: Vec<u32> = g.ports(loop_in, false).iter().map(|p| p.1).collect();
    for (o, i) in &g.links {
        if in_ports.contains(i) && !keep.contains(&(*o, *i)) {
            run("pw-link", &["-d", &o.to_string(), &i.to_string()]).await;
        }
    }

    let Some(stream) = wanted else {
        let mut st = source.state.lock().unwrap();
        st.snapshot.linked = false;
        st.snapshot.error = Some("O app parou de tocar; volta sozinho quando tocar de novo".into());
        return;
    };
    let to_sink = wanted_links(&g, loop_out, sink);
    let mut ok = !keep.is_empty() && !to_sink.is_empty();
    for (o, i) in keep.iter().chain(&to_sink) {
        if !g.links.contains(&(*o, *i))
            && run("pw-link", &[&o.to_string(), &i.to_string()])
                .await
                .is_none()
        {
            ok = false;
        }
    }
    let mut st = source.state.lock().unwrap();
    st.snapshot.linked = ok;
    st.snapshot.error = (!ok).then(|| format!("Não foi possível ligar {}", stream.app));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn port(id: u32, node: u32, ch: &str, dir: &str) -> Value {
        json!({ "id": id, "type": "PipeWire:Interface:Port",
                "info": { "direction": dir, "props": { "node.id": node, "audio.channel": ch } } })
    }

    #[test]
    fn lists_streams_flags_calls_and_pairs_channels() {
        // Um filho de verdade, como o `pw-cat` da voz: o filtro olha o processo pai.
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let dump = json!([
            { "id": 71, "type": "PipeWire:Interface:Node", "info": { "props": {
                "media.class": "Stream/Output/Audio", "application.name": "Firefox (2)",
                "media.name": "Gerudo Valley - YouTube", "application.process.id": 1 } } },
            { "id": 127, "type": "PipeWire:Interface:Node", "info": { "props": {
                "media.class": "Stream/Output/Audio", "application.name": "vesktop",
                "media.name": "Playback", "application.process.binary": "electron" } } },
            { "id": 5, "type": "PipeWire:Interface:Node", "info": { "props": {
                "media.class": "Audio/Sink", "node.name": "fone" } } },
            { "id": 200, "type": "PipeWire:Interface:Node", "info": { "props": {
                "media.class": "Stream/Input/Audio", "node.name": NODE } } },
            { "id": 30, "type": "PipeWire:Interface:Client", "info": { "props": {
                "application.process.id": child.id() } } },
            { "id": 77, "type": "PipeWire:Interface:Node", "info": { "props": {
                "media.class": "Stream/Output/Audio", "node.name": "pw-cat", "client.id": 30 } } },
            { "id": 78, "type": "PipeWire:Interface:Node", "info": { "props": {
                "media.class": "Stream/Output/Audio", "node.name": NODE_OUT } } },
            port(1, 71, "FL", "output"), port(2, 71, "FR", "output"),
            port(3, 200, "FL", "input"), port(4, 200, "FR", "input"),
            port(9, 127, "MONO", "output"),
        ]);
        let g = parse(&dump);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(g.streams.len(), 2);
        assert_eq!(
            g.streams[0].label(),
            "Firefox (2) · Gerudo Valley - YouTube"
        );
        assert!(!g.streams[0].call);
        assert!(g.streams[1].call);
        assert_eq!(g.loop_in, Some(200));
        assert_eq!(wanted_links(&g, 71, 200), vec![(1, 3), (2, 4)]);
        assert_eq!(wanted_links(&g, 127, 200), vec![(9, 3), (9, 4)]);
    }

    #[test]
    fn select_rejects_unknown_and_clears() {
        let s = AppSource::new();
        assert!(s.select(Some(42)).is_err());
        s.select(None).unwrap();
        assert_eq!(s.snapshot().selected, None);
        s.set_volume(500.0);
        assert_eq!(s.snapshot().volume, 100.0);
    }
}
