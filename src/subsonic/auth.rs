use rand::Rng;
use zeroize::Zeroize;

use crate::secret::Secret;

/// Returns `(salt, md5(password + salt))`. Token buffer is zeroized before drop.
///
/// Salt is 16 lowercase-alphanumeric chars; token is the 32-char lowercase hex MD5 digest of `password || salt`.
///
/// ```
/// use ferrosonic::secret::Secret;
/// use ferrosonic::subsonic::auth::generate_auth_params;
/// let pw = Secret::from_string("hunter2".to_string());
/// let (salt, token) = generate_auth_params(&pw);
/// assert_eq!(salt.len(), 16);
/// assert_eq!(token.len(), 32);
/// assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
/// assert!(salt.chars().all(|c| c.is_ascii_alphanumeric()));
/// ```
pub fn generate_auth_params(password: &Secret) -> (String, String) {
    let salt = generate_salt();
    let token = generate_token(password, &salt);
    (salt, token)
}

fn generate_salt() -> String {
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect()
}

fn generate_token(password: &Secret, salt: &str) -> String {
    let mut buf: Vec<u8> =
        Vec::with_capacity(password.reveal_bytes().len() + salt.len());
    buf.extend_from_slice(password.reveal_bytes());
    buf.extend_from_slice(salt.as_bytes());
    let digest = md5::compute(&buf);
    buf.zeroize();
    format!("{:x}", digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token() {
        let pw = Secret::from_string("sesame".to_string());
        let token = generate_token(&pw, "c19b2d");
        assert_eq!(token, "26719a1196d2a940705a59634eb18eab");
    }

    #[test]
    fn test_generate_salt_length() {
        let salt = generate_salt();
        assert_eq!(salt.len(), 16);
    }

    #[test]
    fn test_auth_params() {
        let pw = Secret::from_string("password".to_string());
        let (salt, token) = generate_auth_params(&pw);
        assert_eq!(salt.len(), 16);
        assert_eq!(token.len(), 32);
    }
}
