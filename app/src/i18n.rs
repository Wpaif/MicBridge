//! Textos traduzidos da página do celular.
//!
//! Cada idioma é um JSON plano `chave -> texto` em `web/i18n/`, embutido no
//! binário. O mesmo arquivo serve ao Rust (páginas montadas no servidor) e ao
//! `app.js`, que o recebe embutido na página. Variáveis usam `{nome}`.

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

static LOCALES: LazyLock<Vec<Locale>> = LazyLock::new(|| {
    SOURCES
        .iter()
        .map(|&(tag, json)| Locale {
            tag,
            json,
            texts: serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("web/i18n/{tag}.json inválido: {e}")),
        })
        .collect()
});

fn by_tag(tag: &str) -> Option<&'static Locale> {
    LOCALES.iter().find(|l| l.tag.eq_ignore_ascii_case(tag))
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
        .find_map(|&(_, tag)| {
            by_tag(tag).or_else(|| {
                let lang = tag.split('-').next()?;
                LOCALES.iter().find(|l| {
                    l.tag
                        .split('-')
                        .next()
                        .is_some_and(|p| p.eq_ignore_ascii_case(lang))
                })
            })
        })
        .or_else(|| by_tag(FALLBACK))
        .expect("o idioma padrão existe")
}

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
}
