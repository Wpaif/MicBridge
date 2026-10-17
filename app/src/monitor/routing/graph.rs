use crate::monitor::devices::OutputDevice;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreIdentity {
    pub name: String,
    pub cookie: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectIdentity {
    pub id: u32,
    pub serial: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Input,
    Output,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub node: ObjectIdentity,
    pub node_name: String,
    pub port: ObjectIdentity,
    pub port_name: String,
    pub channel: String,
    pub direction: Direction,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedLink {
    pub core: CoreIdentity,
    pub output: Endpoint,
    pub input: Endpoint,
    pub passive: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedState {
    Present,
    Missing,
    Unavailable,
}
#[derive(Clone, Debug)]
pub struct Graph {
    pub core: CoreIdentity,
    nodes: Vec<(ObjectIdentity, String, String)>,
    ports: Vec<Endpoint>,
    pub links: Vec<SavedLink>,
}
fn number(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}
fn identity(v: &serde_json::Value) -> Option<ObjectIdentity> {
    Some(ObjectIdentity {
        id: u32::try_from(number(&v["id"])?).ok()?,
        serial: number(&v["info"]["props"]["object.serial"])?,
    })
}
impl Graph {
    pub fn parse(json: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let list = value.as_array().ok_or("Grafo PipeWire inválido")?;
        let core = list
            .iter()
            .find(|n| n["type"] == "PipeWire:Interface:Core")
            .ok_or("Core ausente")?;
        let mut g = Self {
            core: CoreIdentity {
                name: core["info"]["name"].as_str().ok_or("Core sem nome")?.into(),
                cookie: u32::try_from(number(&core["info"]["cookie"]).ok_or("Core sem cookie")?)
                    .map_err(|e| e.to_string())?,
            },
            nodes: vec![],
            ports: vec![],
            links: vec![],
        };
        for n in list
            .iter()
            .filter(|n| n["type"] == "PipeWire:Interface:Node")
        {
            if let (Some(id), Some(name)) = (identity(n), n["info"]["props"]["node.name"].as_str())
            {
                g.nodes.push((
                    id,
                    name.into(),
                    n["info"]["props"]["media.class"]
                        .as_str()
                        .unwrap_or("")
                        .into(),
                ));
            }
        }
        for n in list
            .iter()
            .filter(|n| n["type"] == "PipeWire:Interface:Port")
        {
            let p = &n["info"]["props"];
            let Some(port) = identity(n) else { continue };
            let Some((node, name, _)) = g
                .nodes
                .iter()
                .find(|(id, _, _)| Some(id.id as u64) == number(&p["node.id"]))
            else {
                continue;
            };
            let Some(port_name) = p["port.name"].as_str() else {
                continue;
            };
            let direction = match n["info"]["direction"].as_str() {
                Some("input") => Direction::Input,
                Some("output") => Direction::Output,
                _ => continue,
            };
            g.ports.push(Endpoint {
                node: node.clone(),
                node_name: name.clone(),
                port,
                port_name: port_name.into(),
                channel: p["audio.channel"].as_str().unwrap_or("").into(),
                direction,
            });
        }
        for n in list
            .iter()
            .filter(|n| n["type"] == "PipeWire:Interface:Link")
        {
            let info = &n["info"];
            let out = g.ports.iter().find(|p| {
                Some(p.port.id as u64) == number(&info["output-port-id"])
                    && Some(p.node.id as u64) == number(&info["output-node-id"])
                    && p.direction == Direction::Output
            });
            let input = g.ports.iter().find(|p| {
                Some(p.port.id as u64) == number(&info["input-port-id"])
                    && Some(p.node.id as u64) == number(&info["input-node-id"])
                    && p.direction == Direction::Input
            });
            // Missing endpoint identity in a connected graph cannot be acted on safely.
            if let (Some(output), Some(input)) = (out, input) {
                g.links.push(SavedLink {
                    core: g.core.clone(),
                    output: output.clone(),
                    input: input.clone(),
                    passive: info["props"]["link.passive"].as_bool().unwrap_or(false),
                });
            } else if info["state"] != "error" {
                return Err("Link sem identidade verificável dos endpoints".into());
            }
        }
        Ok(g)
    }
    pub fn target_matches(&self, target: &OutputDevice) -> bool {
        self.nodes.iter().any(|(id, name, class)| {
            id.id == target.id
                && target.serial.as_ref().and_then(|s| s.parse::<u64>().ok()) == Some(id.serial)
                && name == &target.name
                && class == "Audio/Sink"
        })
    }
    pub fn return_links(
        &self,
        target: &OutputDevice,
        name: &str,
    ) -> Result<Vec<SavedLink>, String> {
        if !self.target_matches(target) {
            return Err("Saída removida ou substituída".into());
        }
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .filter(|(_, n, c)| n == name && c == "Stream/Output/Audio")
            .collect();
        if nodes.len() != 1 {
            return Err("Fluxo de retorno ainda não disponível".into());
        }
        let outputs: Vec<_> = self
            .ports
            .iter()
            .filter(|p| {
                p.node == nodes[0].0
                    && p.direction == Direction::Output
                    && !p.port_name.starts_with("monitor_")
            })
            .collect();
        let inputs: Vec<_> = self
            .ports
            .iter()
            .filter(|p| p.node.id == target.id && p.direction == Direction::Input)
            .collect();
        if outputs.is_empty() || inputs.is_empty() {
            return Err("Portas de áudio ainda indisponíveis".into());
        }
        let mut links = vec![];
        for input in inputs {
            if !["FL", "FR", "MONO"].contains(&input.channel.as_str()) {
                return Err("Canal de saída não identificável".into());
            }
            let matching: Vec<_> = outputs
                .iter()
                .filter(|p| p.channel == input.channel)
                .collect();
            if matching.len() != 1
                || links
                    .iter()
                    .any(|l: &SavedLink| l.input.channel == input.channel)
            {
                return Err("Mapeamento de canais ambíguo".into());
            }
            links.push(SavedLink {
                core: self.core.clone(),
                output: (*matching[0]).clone(),
                input: input.clone(),
                passive: false,
            });
        }
        Ok(links)
    }
    pub fn competitors(&self, target: &OutputDevice, name: &str) -> Result<Vec<SavedLink>, String> {
        if !self.target_matches(target) {
            return Err("Saída removida ou substituída".into());
        }
        Ok(self
            .links
            .iter()
            .filter(|l| l.input.node.id == target.id && l.output.node_name != name)
            .cloned()
            .collect())
    }
    pub fn validate_saved(&self, link: &SavedLink) -> SavedState {
        if self.core != link.core
            || !self.ports.contains(&link.output)
            || !self.ports.contains(&link.input)
        {
            SavedState::Unavailable
        } else if self
            .links
            .iter()
            .any(|l| l.output == link.output && l.input == link.input)
        {
            SavedState::Present
        } else {
            SavedState::Missing
        }
    }
    #[cfg(test)]
    pub fn remove(&mut self, link: &SavedLink) {
        self.links
            .retain(|l| l.input != link.input || l.output != link.output);
    }
    #[cfg(test)]
    pub fn insert(&mut self, link: SavedLink) {
        if self.validate_saved(&link) == SavedState::Missing {
            self.links.push(link);
        }
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::{Value, json};
    pub fn target() -> OutputDevice {
        OutputDevice {
            id: 1,
            serial: Some("101".into()),
            name: "headphones".into(),
            label: "Fone".into(),
        }
    }
    pub fn fixture() -> Value {
        let mut v = vec![
            json!({"id":0,"type":"PipeWire:Interface:Core","info":{"cookie":777,"name":"pipewire-0"}}),
        ];
        for (id, name, class) in [
            (1, "headphones", "Audio/Sink"),
            (2, "micbridge_monitor_test", "Stream/Output/Audio"),
            (3, "music", "Stream/Output/Audio"),
            (4, "call", "Stream/Output/Audio"),
            (5, "other", "Audio/Sink"),
            (6, "mic", "Audio/Source"),
        ] {
            v.push(json!({"id":id,"type":"PipeWire:Interface:Node","info":{"props":{"object.serial":id+100,"node.name":name,"media.class":class}}}));
            for (offset, channel) in [(1, "FL"), (2, "FR")] {
                let port = id * 10 + offset;
                let direction = if [1, 5].contains(&id) {
                    "input"
                } else {
                    "output"
                };
                v.push(json!({"id":port,"type":"PipeWire:Interface:Port","info":{"direction":direction,"props":{"object.serial":port+100,"node.id":id,"port.name":format!("{direction}_{channel}"),"audio.channel":channel}}}));
            }
        }
        for (id, out, input, passive) in [
            (1000, 21, 11, false),
            (1001, 22, 12, false),
            (1002, 31, 11, false),
            (1003, 32, 12, false),
            (1004, 41, 11, true),
            (1005, 42, 12, true),
            (1006, 31, 51, false),
            (1007, 61, 52, false),
        ] {
            v.push(json!({"id":id,"type":"PipeWire:Interface:Link","info":{"state":"active","output-node-id":out/10,"output-port-id":out,"input-node-id":input/10,"input-port-id":input,"props":{"object.serial":id+100,"link.passive":passive}}}));
        }
        Value::Array(v)
    }
    pub fn graph() -> Graph {
        Graph::parse(&fixture().to_string()).unwrap()
    }
    #[test]
    fn only_selected_sink_competitors_are_removed() {
        let g = graph();
        assert!(g.target_matches(&target()));
        let links = g.competitors(&target(), "micbridge_monitor_test").unwrap();
        assert_eq!(links.len(), 4);
        assert!(
            links
                .iter()
                .all(|l| l.input.node.id == 1 && [3, 4].contains(&l.output.node.id))
        );
        assert_eq!(links.iter().filter(|l| l.passive).count(), 2);
        assert!(
            links
                .iter()
                .all(|l| g.validate_saved(l) == SavedState::Present)
        );
    }
    #[test]
    fn return_mapping_is_verified_before_unlink() {
        let g = graph();
        let links = g.return_links(&target(), "micbridge_monitor_test").unwrap();
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|l| l.output.channel == l.input.channel));
        assert!(
            g.return_links(&target(), "micbridge_monitor_similar")
                .is_err()
        );
        let mut v = fixture();
        v.as_array_mut()
            .unwrap()
            .retain(|n| n["type"] != "PipeWire:Interface:Link");
        let missing = Graph::parse(&v.to_string()).unwrap();
        assert!(
            missing
                .return_links(&target(), "micbridge_monitor_test")
                .unwrap()
                .iter()
                .all(|l| missing.validate_saved(l) == SavedState::Missing)
        );
        for n in v.as_array_mut().unwrap() {
            if n["id"] == 12 {
                n["info"]["props"]["audio.channel"] = json!("FL");
            }
        }
        assert!(
            Graph::parse(&v.to_string())
                .unwrap()
                .return_links(&target(), "micbridge_monitor_test")
                .is_err()
        );
    }
    #[test]
    fn reused_node_port_or_core_identity_is_unavailable() {
        let saved = graph()
            .competitors(&target(), "micbridge_monitor_test")
            .unwrap()
            .remove(0);
        for (id, path) in [(3, "node"), (31, "port"), (0, "core")] {
            let mut v = fixture();
            for n in v.as_array_mut().unwrap() {
                if n["id"] == id {
                    if path == "core" {
                        n["info"]["cookie"] = json!(778);
                    } else {
                        n["info"]["props"]["object.serial"] = json!(9000);
                    }
                }
            }
            assert_eq!(
                Graph::parse(&v.to_string()).unwrap().validate_saved(&saved),
                SavedState::Unavailable
            );
        }
        assert!(Graph::parse("[").is_err());
        assert!(Graph::parse("{}").is_err());
        let mut v = fixture();
        v[1]["info"]["props"]["object.serial"] = json!("101");
        assert!(
            Graph::parse(&v.to_string())
                .unwrap()
                .target_matches(&target())
        );
    }
}
