use axum::{
    body::Body,
    http::Request,
    response::Response,
    middleware::Next,
};

use jsonwebtoken::{decode, DecodingKey, Validation};

use crate::Claims;

pub async fn auth(
    req: Request<Body>,
    next: Next,
) -> Response {
    let token = req.headers().get("Authorization").and_then(|header| header.to_str().ok()).and_then(|header| header.strip_prefix("Bearer "));

    if let Some(token) = token {
        let decoding_key = DecodingKey::from_secret("secret".as_ref());
        if let Ok(_token_data) = decode::<Claims>(token, &decoding_key, &Validation::default()) {
            // You can add the claims to the request extensions for later use
            // req.extensions_mut().insert(token_data.claims);
            return next.run(req).await;
        }
    }

    Response::builder()
        .status(401)
        .body("Unauthorized".into())
        .unwrap()
}
