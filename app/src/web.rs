//! Páginas e arquivos estáticos servidos ao celular (substitui o antigo servidor Rails).
//!
//! O celular é reconhecido por um cookie `_micbridge=<id>.<segredo>` (HttpOnly,
//! Secure, SameSite=Strict), criado ao escanear o QR code com a janela de
//! pareamento aberta.

use crate::server::AppState;
use crate::state::ct_eq;
use crate::tls;
use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::collections::HashMap;
use std::sync::Arc;

const COOKIE: &str = "_micbridge";
const COOKIE_MAX_AGE: u32 = 365 * 24 * 3600;

const HEAD: &str = include_str!("../web/head.html");
const INDEX: &str = include_str!("../web/index.html");
const DENIED: &str = include_str!("../web/denied.html");

const MSG_DEFAULT: &str = "Acesso não autorizado.<br>Escaneie o QR code mostrado no app do PC.";
const MSG_UNPAIRED: &str =
    "Este celular não está pareado. Escaneie o QR code mostrado no app do PC.";
const MSG_CLOSED: &str = "Pareamento fechado. Ative \"Aceitar novo celular\" no app do PC.";
const MSG_BAD_QR: &str = "QR code inválido.<br>Escaneie o QR code mostrado no app do PC.";

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(index))
        .route("/ca.crt", get(ca))
        .route(
            "/app.js",
            get(|| asset("text/javascript", include_str!("../web/app.js"))),
        )
        .route(
            "/style.css",
            get(|| asset("text/css", include_str!("../web/style.css"))),
        )
        .route(
            "/manifest.json",
            get(|| {
                asset(
                    "application/manifest+json",
                    include_str!("../web/manifest.json"),
                )
            }),
        )
        .route(
            "/icon.svg",
            get(|| asset("image/svg+xml", include_str!("../web/icon.svg"))),
        )
        .route("/icon.png", get(icon_png))
}

/// Credenciais `(id, segredo)` do cookie, já validadas quanto ao formato.
pub fn credentials(h: &HeaderMap) -> Option<(String, String)> {
    let raw = h.get(header::COOKIE)?.to_str().ok()?;
    let value = raw
        .split(';')
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)?
        .1;
    let (id, secret) = value.split_once('.')?;
    let hex = |s: &str| !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_hexdigit());
    (hex(id) && hex(secret)).then(|| (id.to_string(), secret.to_string()))
}

fn session_cookie(id: &str, secret: &str) -> String {
    format!(
        "{COOKIE}={id}.{secret}; Path=/; Max-Age={COOKIE_MAX_AGE}; Secure; HttpOnly; SameSite=Strict"
    )
}

fn clear_cookie() -> String {
    format!("{COOKIE}=; Path=/; Max-Age=0; Secure; HttpOnly; SameSite=Strict")
}

/// Cabeçalhos comuns: sem cache, sem sniffing e CSP restrita ao próprio servidor.
fn secured(mut res: Response) -> Response {
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; \
             connect-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'none'",
        ),
    );
    res
}

fn html(status: StatusCode, body: String, cookie: Option<String>) -> Response {
    let mut res = (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response();
    if let Some(c) = cookie.and_then(|c| HeaderValue::from_str(&c).ok()) {
        res.headers_mut().append(header::SET_COOKIE, c);
    }
    secured(res)
}

fn denied(message: &str, cookie: Option<String>) -> Response {
    let body = DENIED
        .replace("<!--HEAD-->", HEAD)
        .replace("<!--MESSAGE-->", message);
    html(StatusCode::FORBIDDEN, body, cookie)
}

async fn asset(content_type: &'static str, body: &'static str) -> Response {
    secured(([(header::CONTENT_TYPE, content_type)], body).into_response())
}

async fn icon_png() -> Response {
    secured(
        (
            [(header::CONTENT_TYPE, "image/png")],
            include_bytes!("../web/icon.png").as_slice(),
        )
            .into_response(),
    )
}

/// CA local para o celular instalar e passar a confiar no servidor.
async fn ca() -> Response {
    match std::fs::read(tls::ca_path()) {
        Ok(pem) => secured(
            (
                [
                    (header::CONTENT_TYPE, "application/x-x509-ca-cert"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=\"micbridge-ca.crt\"",
                    ),
                ],
                pem,
            )
                .into_response(),
        ),
        Err(_) => secured((StatusCode::NOT_FOUND, "CA não encontrada").into_response()),
    }
}

async fn index(
    State(st): State<Arc<AppState>>,
    Query(q): Query<HashMap<String, String>>,
    h: HeaderMap,
) -> Response {
    // Pareamento: o QR traz `?t=<token>`; troca por um cookie de longa duração.
    if let Some(token) = q.get("t").filter(|t| !t.is_empty()) {
        // Celular já pareado que releu o QR só para abrir a interface: não precisa da janela.
        if credentials(&h).is_some_and(|(id, secret)| st.shared.verify_device(&id, &secret)) {
            return secured((StatusCode::SEE_OTHER, [(header::LOCATION, "/")]).into_response());
        }
        if st.shared.pairing_remaining() == 0 {
            return denied(MSG_CLOSED, None);
        }
        if !ct_eq(token, &st.shared.pair_token) {
            return denied(MSG_BAD_QR, None);
        }
        let (id, secret) = st.shared.add_device();
        tracing::info!("novo celular pareado");
        // Redireciona para tirar o token da URL (histórico, favoritos).
        let mut res = (StatusCode::SEE_OTHER, [(header::LOCATION, "/")]).into_response();
        if let Ok(c) = HeaderValue::from_str(&session_cookie(&id, &secret)) {
            res.headers_mut().append(header::SET_COOKIE, c);
        }
        return secured(res);
    }

    match credentials(&h) {
        None => denied(MSG_DEFAULT, None),
        Some((id, secret)) if st.shared.verify_device(&id, &secret) => {
            html(StatusCode::OK, INDEX.replace("<!--HEAD-->", HEAD), None)
        }
        // Cookie de um celular que foi esquecido no app: limpa e orienta a parear de novo.
        Some(_) => denied(MSG_UNPAIRED, Some(clear_cookie())),
    }
}
