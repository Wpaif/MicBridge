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
}
