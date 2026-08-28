use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const CAPABILITY_PREFIX: &str = "clared-cap-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityClaims {
    pub session_id: String,
    pub tenant_id: String,
    pub principal: String,
    pub generation: u64,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
    pub jti: String,
}

pub struct CapabilitySigner {
    signing_key: SigningKey,
}

impl CapabilitySigner {
    pub fn random() -> Self {
        Self {
            signing_key: SigningKey::generate(&mut OsRng),
        }
    }

    #[cfg(test)]
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn issue(&self, claims: &CapabilityClaims) -> Result<String, String> {
        let payload = serde_json::to_vec(claims)
            .map_err(|error| format!("Capability serialization failed: {error}"))?;
        let signature = self.signing_key.sign(&payload);

        Ok(format!(
            "{}.{}.{}",
            CAPABILITY_PREFIX,
            URL_SAFE_NO_PAD.encode(payload),
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        ))
    }

    pub fn verify(&self, token: &str) -> Result<CapabilityClaims, String> {
        let mut parts = token.split('.');
        let prefix = parts.next().ok_or("Malformed capability token")?;
        let payload_b64 = parts.next().ok_or("Malformed capability token")?;
        let signature_b64 = parts.next().ok_or("Malformed capability token")?;
        if parts.next().is_some() || prefix != CAPABILITY_PREFIX {
            return Err("Unsupported or malformed capability token".to_string());
        }

        let payload = URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| "Capability payload is not valid base64url".to_string())?;
        let signature_bytes = URL_SAFE_NO_PAD
            .decode(signature_b64)
            .map_err(|_| "Capability signature is not valid base64url".to_string())?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|_| "Capability signature has an invalid length".to_string())?;

        self.signing_key
            .verifying_key()
            .verify(&payload, &signature)
            .map_err(|_| "Capability signature verification failed".to_string())?;

        serde_json::from_slice(&payload)
            .map_err(|error| format!("Capability claims are invalid: {error}"))
    }

    pub fn sign_evidence(&self, evidence: &Value) -> Result<(String, String), String> {
        let bytes = serde_json::to_vec(evidence)
            .map_err(|error| format!("Evidence serialization failed: {error}"))?;
        let digest = Sha256::digest(&bytes);
        let signature = self.signing_key.sign(&digest);

        Ok((
            format!("sha256:{}", hex::encode(digest)),
            format!("ed25519:{}", URL_SAFE_NO_PAD.encode(signature.to_bytes())),
        ))
    }

    pub fn public_key_base64(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.signing_key.verifying_key().to_bytes())
    }

    pub fn verify_evidence(
        public_key_base64: &str,
        evidence: &Value,
        signature_value: &str,
    ) -> Result<(), String> {
        let public_key_bytes = URL_SAFE_NO_PAD
            .decode(public_key_base64)
            .map_err(|_| "Evidence public key is not valid base64url".to_string())?;
        let public_key_array: [u8; 32] = public_key_bytes
            .try_into()
            .map_err(|_| "Evidence public key must be 32 bytes".to_string())?;
        let public_key = VerifyingKey::from_bytes(&public_key_array)
            .map_err(|_| "Evidence public key is invalid".to_string())?;

        let encoded_signature = signature_value
            .strip_prefix("ed25519:")
            .ok_or("Evidence signature prefix is invalid")?;
        let signature_bytes = URL_SAFE_NO_PAD
            .decode(encoded_signature)
            .map_err(|_| "Evidence signature is not valid base64url".to_string())?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|_| "Evidence signature has an invalid length".to_string())?;
        let bytes = serde_json::to_vec(evidence)
            .map_err(|error| format!("Evidence serialization failed: {error}"))?;
        let digest = Sha256::digest(&bytes);

        public_key
            .verify(&digest, &signature)
            .map_err(|_| "Evidence signature verification failed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capability_round_trip_and_tamper_rejection() {
        let signer = CapabilitySigner::from_seed([7_u8; 32]);
        let claims = CapabilityClaims {
            session_id: "ses_test".to_string(),
            tenant_id: "acme".to_string(),
            principal: "alice".to_string(),
            generation: 1,
            issued_at_ms: 10,
            expires_at_ms: 20,
            jti: "jti_test".to_string(),
        };
        let token = signer.issue(&claims).unwrap();
        assert_eq!(signer.verify(&token).unwrap(), claims);

        let mut tampered = token.into_bytes();
        let last = tampered.len() - 1;
        tampered[last] = if tampered[last] == b'A' { b'B' } else { b'A' };
        assert!(signer
            .verify(&String::from_utf8(tampered).unwrap())
            .is_err());
    }

    #[test]
    fn evidence_signature_round_trip() {
        let signer = CapabilitySigner::from_seed([9_u8; 32]);
        let evidence = json!({"session_id": "ses_test", "actions": []});
        let (_, signature) = signer.sign_evidence(&evidence).unwrap();
        CapabilitySigner::verify_evidence(&signer.public_key_base64(), &evidence, &signature)
            .unwrap();
    }
}
