-- Seed a second login-capable user with role user
INSERT INTO users (username, email, password_hash, role)
VALUES (
    'demo',
    'demo@test.com',
    '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYfQYgkEzPe',
    'user'
)
ON CONFLICT (username) DO NOTHING;
