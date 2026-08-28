use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use rand::{distributions::Alphanumeric, Rng};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;
const DELEGATION_PREFIX: &str = "clared-delegation-v1";

fn delegation_message(
    tenant_id: &str,
    principal: &str,
    agent_role: &str,
    task_intent: &str,
    expires_at_ms: i64,
    nonce: &str,
) -> Result<String, String> {
    for value in [tenant_id, principal, agent_role, task_intent, nonce] {
        if value.contains('\u{1f}') {
            return Err("Delegation fields may not contain the unit separator".to_string());
        }
    }

    Ok(format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        tenant_id, principal, agent_role, task_intent, expires_at_ms, nonce
    ))
}

pub fn issue_delegation_token(
    secret: &[u8],
    tenant_id: &str,
    principal: &str,
    agent_role: &str,
    task_intent: &str,
    expires_at_ms: i64,
) -> Result<String, String> {
    if secret.len() < 32 {
        return Err("Delegation secret must contain at least 32 bytes".to_string());
    }
    let nonce: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();
    let message = delegation_message(
        tenant_id,
        principal,
        agent_role,
        task_intent,
        expires_at_ms,
        &nonce,
    )?;
    let mut mac = HmacSha256::new_from_slice(secret)
        .map_err(|_| "Delegation secret is invalid".to_string())?;
    mac.update(message.as_bytes());

    Ok(format!(
        "{}.{}.{}.{}",
        DELEGATION_PREFIX,
        expires_at_ms,
        nonce,
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    ))
}

pub fn verify_delegation_token(
    secret: &[u8],
    token: &str,
    tenant_id: &str,
    principal: &str,
    agent_role: &str,
    task_intent: &str,
    now_ms: i64,
) -> Result<(), String> {
    let mut parts = token.split('.');
    let prefix = parts.next().ok_or("Malformed delegation token")?;
    let expires_at_ms = parts
        .next()
        .ok_or("Malformed delegation token")?
        .parse::<i64>()
        .map_err(|_| "Delegation expiry is invalid".to_string())?;
    let nonce = parts.next().ok_or("Malformed delegation token")?;
    let provided_mac = parts.next().ok_or("Malformed delegation token")?;
    if parts.next().is_some() || prefix != DELEGATION_PREFIX {
        return Err("Unsupported or malformed delegation token".to_string());
    }
    if expires_at_ms <= now_ms {
        return Err("Delegation token has expired".to_string());
    }

    let message = delegation_message(
        tenant_id,
        principal,
        agent_role,
        task_intent,
        expires_at_ms,
        nonce,
    )?;
    let mac_bytes = URL_SAFE_NO_PAD
        .decode(provided_mac)
        .map_err(|_| "Delegation MAC is not valid base64url".to_string())?;
    let mut mac = HmacSha256::new_from_slice(secret)
        .map_err(|_| "Delegation secret is invalid".to_string())?;
    mac.update(message.as_bytes());
    mac.verify_slice(&mac_bytes)
        .map_err(|_| "Delegation token verification failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegation_is_bound_to_identity_and_intent() {
        let secret = b"0123456789abcdef0123456789abcdef";
        let token =
            issue_delegation_token(secret, "acme", "alice", "support", "resolve_dispute", 2_000)
                .unwrap();

        verify_delegation_token(
            secret,
            &token,
            "acme",
            "alice",
            "support",
            "resolve_dispute",
            1_000,
        )
        .unwrap();
        assert!(verify_delegation_token(
            secret,
            &token,
            "acme",
            "mallory",
            "support",
            "resolve_dispute",
            1_000,
        )
        .is_err());
    }
}
