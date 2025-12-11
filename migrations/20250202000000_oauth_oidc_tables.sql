-- OAuth2/OIDC core tables
CREATE TABLE IF NOT EXISTS oauth_clients (
    client_id TEXT PRIMARY KEY,
    client_secret TEXT NOT NULL,
    redirect_uris TEXT NOT NULL, -- CSV de URIs permitidas
    scopes TEXT NOT NULL DEFAULT 'openid profile email',
    grant_types TEXT NOT NULL DEFAULT 'authorization_code,refresh_token,client_credentials',
    name TEXT NOT NULL DEFAULT 'default client',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS oauth_authorization_codes (
    code TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth_clients(client_id),
    username TEXT NOT NULL REFERENCES users(username),
    redirect_uri TEXT NOT NULL,
    scope TEXT NOT NULL,
    code_challenge TEXT,
    code_challenge_method TEXT,
    nonce TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS oauth_refresh_tokens (
    refresh_token TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth_clients(client_id),
    username TEXT NOT NULL REFERENCES users(username),
    scope TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE IF NOT EXISTS oauth_jwks (
    kid TEXT PRIMARY KEY,
    alg TEXT NOT NULL,
    kty TEXT NOT NULL,
    n TEXT NOT NULL,
    e TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    active BOOLEAN NOT NULL DEFAULT TRUE
);

-- Seed: cliente demo para pruebas locales
INSERT INTO oauth_clients (client_id, client_secret, redirect_uris, scopes, name)
VALUES (
    'demo-client',
    'demo-secret',
    'http://localhost:3000/callback,http://127.0.0.1:3000/callback',
    'openid profile email offline_access',
    'Demo Local Client'
)
ON CONFLICT (client_id) DO NOTHING;
