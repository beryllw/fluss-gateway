use base64::Engine;

/// Credentials extracted from HTTP Basic Auth.
#[derive(Clone, Debug)]
pub struct BasicAuthCredentials {
    pub username: String,
    pub password: String,
}

pub(crate) fn parse_basic_auth(header_value: &str) -> Option<BasicAuthCredentials> {
    if !header_value.starts_with("Basic ") {
        return None;
    }
    let encoded = &header_value[6..];
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let colon_pos = decoded_str.find(':')?;
    let username = decoded_str[..colon_pos].to_string();
    let password = decoded_str[colon_pos + 1..].to_string();

    if username.is_empty() {
        return None;
    }

    Some(BasicAuthCredentials { username, password })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_auth() {
        let creds = parse_basic_auth("Basic dXNlcjpwYXNz").unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass");
    }

    #[test]
    fn test_parse_basic_auth_invalid() {
        assert!(parse_basic_auth("Bearer token").is_none());
        assert!(parse_basic_auth("Basic notbase64").is_none());
        assert!(parse_basic_auth("Basic dXNlcg==").is_none()); // no colon
    }

    #[test]
    fn test_parse_basic_auth_empty_user() {
        assert!(parse_basic_auth("Basic OnBhc3M=").is_none()); // :pass
    }
}
