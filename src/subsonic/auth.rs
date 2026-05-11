use rand::Rng;

/// Returns `(salt, md5(password + salt))`.
pub fn generate_auth_params(password: &str) -> (String, String) {
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

fn generate_token(password: &str, salt: &str) -> String {
    let input = format!("{}{}", password, salt);
    let digest = md5::compute(input.as_bytes());
    format!("{:x}", digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token() {
        let token = generate_token("sesame", "c19b2d");
        assert_eq!(token, "26719a1196d2a940705a59634eb18eab");
    }

    #[test]
    fn test_generate_salt_length() {
        let salt = generate_salt();
        assert_eq!(salt.len(), 16);
    }

    #[test]
    fn test_auth_params() {
        let (salt, token) = generate_auth_params("password");
        assert_eq!(salt.len(), 16);
        assert_eq!(token.len(), 32);
    }
}
