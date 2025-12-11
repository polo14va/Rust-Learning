pub const LOGIN_TEMPLATE: &str = include_str!("login.html");
pub const CONSENT_TEMPLATE: &str = include_str!("consent.html");

pub fn render_login_page(next_hidden: &str, error_html: &str) -> String {
    LOGIN_TEMPLATE
        .replace("{{NEXT_HIDDEN}}", next_hidden)
        .replace("{{ERROR_HTML}}", error_html)
}

pub fn render_consent_page(
    client_name: &str,
    username: &str,
    scopes_html: &str,
    client_id: &str,
    redirect_uri: &str,
    scope_value: &str,
    state_input: &str,
    code_challenge_input: &str,
    code_method_input: &str,
    nonce_input: &str,
) -> String {
    CONSENT_TEMPLATE
        .replace("{{CLIENT_NAME}}", client_name)
        .replace("{{USERNAME}}", username)
        .replace("{{SCOPES}}", scopes_html)
        .replace("{{CLIENT_ID}}", client_id)
        .replace("{{REDIRECT_URI}}", redirect_uri)
        .replace("{{SCOPE_VALUE}}", scope_value)
        .replace("{{STATE_INPUT}}", state_input)
        .replace("{{CODE_CHALLENGE_INPUT}}", code_challenge_input)
        .replace("{{CODE_METHOD_INPUT}}", code_method_input)
        .replace("{{NONCE_INPUT}}", nonce_input)
}
