//! Textos traduzidos.
//!
//! Cada idioma é um JSON plano `chave -> texto`, embutido no binário, com
//! variáveis em `{nome}`. Há dois conjuntos, porque são públicos diferentes:
//!
//! - a página do celular (`web/i18n/`), no idioma do navegador de quem a abre;
//!   o mesmo arquivo vai embutido na página para o `app.js`;
//! - o PC (`ui/i18n/`), no idioma do sistema: a janela QML lê o arquivo, e o
//!   daemon usa as chaves `daemon.*` para o que mostra na janela e na bandeja.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Idioma usado quando o navegador não pede nenhum dos disponíveis.
const FALLBACK: &str = "en";

/// `(tag BCP 47, JSON)` de cada idioma disponível.
const SOURCES: &[(&str, &str)] = &[
    ("pt-BR", include_str!("../web/i18n/pt-BR.json")),
    ("en", include_str!("../web/i18n/en.json")),
];

pub struct Locale {
    /// Tag para `<html lang>`.
    pub tag: &'static str,
    /// JSON original, repassado ao `app.js`.
    pub json: &'static str,
    texts: HashMap<String, String>,
}

impl Locale {
    /// Texto da chave; a própria chave quando ela falta (os testes impedem isso).
    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.texts.get(key).map_or(key, String::as_str)
    }
}

fn load(dir: &str, sources: &[(&'static str, &'static str)]) -> Vec<Locale> {
    sources
        .iter()
        .map(|&(tag, json)| Locale {
            tag,
            json,
            texts: serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("{dir}/{tag}.json inválido: {e}")),
        })
        .collect()
}

static LOCALES: LazyLock<Vec<Locale>> = LazyLock::new(|| load("web/i18n", SOURCES));

fn by_tag(tag: &str) -> Option<&'static Locale> {
    find(&LOCALES, tag)
}

/// A tag exata (`pt-BR`) ou, sem ela, a mesma língua (`pt-PT` e `pt` caem em `pt-BR`).
fn find<'a>(locales: &'a [Locale], tag: &str) -> Option<&'a Locale> {
    locales
        .iter()
        .find(|l| l.tag.eq_ignore_ascii_case(tag))
        .or_else(|| {
            let lang = tag.split('-').next()?;
            locales.iter().find(|l| {
                l.tag
                    .split('-')
                    .next()
                    .is_some_and(|p| p.eq_ignore_ascii_case(lang))
            })
        })
}

/// Escolhe o idioma pelo cabeçalho `Accept-Language` (ex.: `pt-BR,pt;q=0.9,en;q=0.8`).
///
/// Respeita os pesos `q`. Para cada idioma pedido, tenta a tag exata e depois só
/// a língua (`pt-PT` e `pt` caem em `pt-BR`). Sem acerto, usa [`FALLBACK`].
pub fn negotiate(accept_language: Option<&str>) -> &'static Locale {
    let mut wanted: Vec<(f32, &str)> = accept_language
        .unwrap_or("")
        .split(',')
        .filter_map(|part| {
            let mut fields = part.split(';');
            let tag = fields.next()?.trim();
            let q = fields
                .find_map(|f| f.trim().strip_prefix("q="))
                .map_or(Some(1.0), |q| q.trim().parse().ok())?;
            (!tag.is_empty() && tag != "*" && q > 0.0).then_some((q, tag))
        })
        .collect();
    // Ordenação estável: com pesos iguais, vale a ordem do cabeçalho.
    wanted.sort_by(|a, b| b.0.total_cmp(&a.0));

    wanted
        .iter()
        .find_map(|&(_, tag)| by_tag(tag))
        .or_else(|| by_tag(FALLBACK))
        .expect("o idioma padrão existe")
}

/// `(tag, JSON)` dos idiomas do PC, os mesmos que o `I18n.qml` carrega.
const DESKTOP_SOURCES: &[(&str, &str)] = &[
    ("pt-BR", include_str!("../ui/i18n/pt-BR.json")),
    ("en", include_str!("../ui/i18n/en.json")),
];

static DESKTOP_LOCALES: LazyLock<Vec<Locale>> = LazyLock::new(|| load("ui/i18n", DESKTOP_SOURCES));

/// Idioma de uma variável POSIX (`pt_BR.UTF-8`, `en_US`, `C`), como o `I18n.qml` faz.
fn posix(locale: &str) -> &'static Locale {
    let tag = locale
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    find(&DESKTOP_LOCALES, &tag)
        .or_else(|| find(&DESKTOP_LOCALES, FALLBACK))
        .expect("o idioma padrão existe")
}

static DESKTOP: LazyLock<&'static Locale> = LazyLock::new(|| {
    // Os testes conferem os textos em português, qualquer que seja o idioma da máquina.
    if cfg!(test) {
        return posix("pt_BR");
    }
    let var = |name| std::env::var(name).ok().filter(|v| !v.is_empty());
    posix(
        &var("LC_ALL")
            .or_else(|| var("LC_MESSAGES"))
            .or_else(|| var("LANG"))
            .unwrap_or_default(),
    )
});

/// Idioma do PC, para o que o daemon mostra na janela e na bandeja.
pub fn desktop() -> &'static Locale {
    *DESKTOP
}

/// Textos da chave em todos os idiomas do PC (para comparar com o que o usuário digita).
pub fn desktop_all(key: &'static str) -> impl Iterator<Item = &'static str> {
    DESKTOP_LOCALES.iter().map(move |l| l.t(key))
}

/// Número com o separador decimal do idioma do PC.
pub fn num(value: f64, digits: usize) -> String {
    format!("{value:.digits$}").replace('.', desktop().t("format.decimal"))
}

impl Locale {
    /// Texto da chave com as variáveis `{nome}` trocadas pelos valores.
    pub fn fill(&self, key: &str, vars: &[(&str, &dyn std::fmt::Display)]) -> String {
        let mut text = self.t(key).to_string();
        for (name, value) in vars {
            text = text.replace(&format!("{{{name}}}"), &value.to_string());
        }
        text
    }
}

/// Texto no idioma do PC: `t!("chave")` dá `&'static str`;
/// `t!("chave", nome = valor, ...)` troca as variáveis e dá `String`.
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::desktop().t($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::desktop().fill($key, &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+])
    };
}
pub(crate) use t;

/// Escapa texto para HTML e transforma `\n` em `<br>`.
pub fn html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out
}

/// JSON seguro dentro de `<script type="application/json">`: um `</script>`
/// num texto não pode fechar o bloco antes da hora.
pub fn script_json(json: &str) -> String {
    json.replace('<', "\\u003c")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn vars(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|s| s.split_once('}'))
            .map(|(v, _)| v)
            .collect()
    }

    #[test]
    fn todos_os_idiomas_tem_as_mesmas_chaves_e_variaveis() {
        let base = by_tag("pt-BR").unwrap();
        for locale in LOCALES.iter() {
            let keys: BTreeSet<_> = locale.texts.keys().collect();
            let base_keys: BTreeSet<_> = base.texts.keys().collect();
            assert_eq!(keys, base_keys, "chaves diferentes em {}", locale.tag);
            for (key, text) in &base.texts {
                assert_eq!(
                    vars(locale.t(key)),
                    vars(text),
                    "variáveis diferentes em {}: {key}",
                    locale.tag
                );
            }
        }
    }

    #[test]
    fn negocia_pelo_accept_language() {
        assert_eq!(negotiate(Some("pt-BR,pt;q=0.9,en;q=0.8")).tag, "pt-BR");
        assert_eq!(negotiate(Some("pt-PT")).tag, "pt-BR");
        assert_eq!(negotiate(Some("en-US,en;q=0.9")).tag, "en");
        assert_eq!(negotiate(Some("de-DE,en;q=0.5,pt;q=0.8")).tag, "pt-BR");
        assert_eq!(negotiate(Some("pt;q=0, en")).tag, "en");
        assert_eq!(negotiate(Some("de, fr")).tag, FALLBACK);
        assert_eq!(negotiate(Some("*")).tag, FALLBACK);
        assert_eq!(negotiate(None).tag, FALLBACK);
    }

    #[test]
    fn escapa_html_e_quebra_linhas() {
        assert_eq!(
            html("a <b> & \"c\"\nd"),
            "a &lt;b&gt; &amp; &quot;c&quot;<br>d"
        );
        assert_eq!(
            script_json(r#"{"x":"</script>"}"#),
            r#"{"x":"\u003c/script>"}"#
        );
    }

    /// Catálogos da janela (ui/i18n), carregados pelo I18n.qml.
    const UI_SOURCES: &[(&str, &str)] = &[
        ("pt-BR", include_str!("../ui/i18n/pt-BR.json")),
        ("en", include_str!("../ui/i18n/en.json")),
    ];

    /// Arquivos QML que podem chamar I18n.t/plural.
    const UI_QML: &[(&str, &str)] = &[
        ("shell.qml", include_str!("../ui/shell.qml")),
        ("Confirm.qml", include_str!("../ui/Confirm.qml")),
        ("NameField.qml", include_str!("../ui/NameField.qml")),
        ("History.qml", include_str!("../ui/History.qml")),
        ("Fader.qml", include_str!("../ui/Fader.qml")),
        ("Slider.qml", include_str!("../ui/Slider.qml")),
        ("I18n.qml", include_str!("../ui/I18n.qml")),
    ];

    fn ui_catalogs() -> Vec<(&'static str, HashMap<String, String>)> {
        UI_SOURCES
            .iter()
            .map(|&(tag, json)| {
                let texts = serde_json::from_str(json)
                    .unwrap_or_else(|e| panic!("ui/i18n/{tag}.json inválido: {e}"));
                (tag, texts)
            })
            .collect()
    }

    /// Chaves literais em `I18n.t("...")` (e `t("...")` dentro do I18n.qml);
    /// as de `I18n.plural("x", ...)` viram `x.one` e `x.other`.
    fn ui_keys(qml: &str) -> Vec<String> {
        let mut keys = Vec::new();
        for (open, plural) in [("t(\"", false), ("plural(\"", true)] {
            let mut rest = qml;
            while let Some(i) = rest.find(open) {
                // `t(` precisa ser a função inteira, não o fim de `effect(`.
                let before = qml[..qml.len() - rest.len() + i].chars().next_back();
                rest = &rest[i + open.len()..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let Some(end) = rest.find('"') else { break };
                let key = &rest[..end];
                if plural {
                    keys.push(format!("{key}.one"));
                    keys.push(format!("{key}.other"));
                } else {
                    keys.push(key.to_string());
                }
            }
        }
        keys
    }

    #[test]
    fn janela_tem_as_mesmas_chaves_e_variaveis_em_todos_os_idiomas() {
        let catalogs = ui_catalogs();
        let (_, base) = &catalogs[0];
        for (tag, texts) in &catalogs {
            let keys: BTreeSet<_> = texts.keys().collect();
            assert_eq!(
                keys,
                base.keys().collect(),
                "chaves diferentes em ui/i18n/{tag}.json"
            );
            for (key, text) in base {
                assert_eq!(
                    vars(&texts[key]),
                    vars(text),
                    "variáveis diferentes em {tag}: {key}"
                );
            }
        }
    }

    #[test]
    fn toda_chave_usada_na_janela_tem_texto() {
        let keys: Vec<String> = UI_QML.iter().flat_map(|(_, qml)| ui_keys(qml)).collect();
        for expected in [
            "page.voz",
            "pair.devices.one",
            "format.decimal",
            "common.cancel",
        ] {
            assert!(
                keys.iter().any(|k| k == expected),
                "{expected} não extraída"
            );
        }
        for (tag, texts) in ui_catalogs() {
            for key in &keys {
                assert!(
                    texts.contains_key(key),
                    "ui/i18n/{tag}.json sem a chave {key}"
                );
            }
        }
    }

    /// Chaves `t!("...")` e `"daemon.*"` literais no código de produção do Rust.
    fn rust_keys() -> Vec<String> {
        fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let src = std::fs::read_to_string(&path).unwrap();
                    let code = src.split("#[cfg(test)]").next().unwrap();
                    let mut rest = code;
                    while let Some(i) = rest.find("\"daemon.") {
                        rest = &rest[i + 1..];
                        let end = rest.find('"').unwrap();
                        out.push(rest[..end].to_string());
                    }
                }
            }
        }
        let mut keys = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut keys,
        );
        keys
    }

    #[test]
    fn toda_chave_do_daemon_tem_texto_nos_idiomas_do_pc() {
        let keys = rust_keys();
        for expected in [
            "daemon.status.connected",
            "daemon.tray.quit",
            "daemon.eq.flat",
            "daemon.preset.bruto.name",
        ] {
            assert!(
                keys.iter().any(|k| k == expected),
                "{expected} não extraída"
            );
        }
        for locale in DESKTOP_LOCALES.iter() {
            for key in &keys {
                assert_ne!(
                    locale.t(key),
                    key.as_str(),
                    "ui/i18n/{}.json sem a chave {key}",
                    locale.tag
                );
            }
        }
    }

    #[test]
    fn idioma_do_pc_vem_da_variavel_posix() {
        assert_eq!(posix("pt_BR.UTF-8").tag, "pt-BR");
        assert_eq!(posix("pt_PT.UTF-8@euro").tag, "pt-BR");
        assert_eq!(posix("en_US.UTF-8").tag, "en");
        assert_eq!(posix("de_DE.UTF-8").tag, FALLBACK);
        assert_eq!(posix("C").tag, FALLBACK);
        assert_eq!(posix("").tag, FALLBACK);
        // Nos testes o daemon fala português, para os textos conferidos nos outros módulos.
        assert_eq!(desktop().tag, "pt-BR");
        assert_eq!(num(-1.25, 1), "-1,2");
    }

    #[test]
    fn daemon_em_ingles_usa_textos_e_variaveis_do_catalogo() {
        let en = posix("en_US.UTF-8");
        assert_eq!(en.t("daemon.status.connected"), "Connected");
        assert_eq!(en.t("daemon.tray.quit"), "Quit");
        assert_eq!(
            en.fill("daemon.monitor.active", &[("device", &"Headphones")]),
            "Exclusive monitor on Headphones"
        );
        assert_eq!(en.t("format.decimal"), ".");
        assert_eq!(en.t("export.default_name"), "test-original.wav");
    }

    #[test]
    fn nome_de_fabrica_em_qualquer_idioma_nao_pode_virar_perfil() {
        let library = crate::profiles::Library::new(crate::dsp::Preset::Chamada);
        for name in ["Chamada", "call", "Original"] {
            assert!(
                library.name(name, None).is_err(),
                "{name} deveria conflitar"
            );
        }
        assert_eq!(library.name("Minha voz", None).unwrap(), "Minha voz");
    }
}
