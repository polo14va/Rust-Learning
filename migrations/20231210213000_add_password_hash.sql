-- Add password_hash to users table
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_hash VARCHAR(255) NOT NULL DEFAULT '$2a$12$mqL.e8a.n9.u8.u8.u8.u8.u8.u8.u8.u8.u8.u8.u8.u8.u8.u8';

-- NOTE: The default hash above is a placeholder. 
-- In a real app, you would force users to reset password or use a migration script to hash existing passwords.
-- For this learning project, all existing users will have this invalid hash (nobody can login as them yet).

-- Create a test user with username 'admin' and password 'test123'
-- Hash generated with: bcrypt::hash("test123", bcrypt::DEFAULT_COST)
INSERT INTO users (username, email, password_hash) 
VALUES ('admin', 'admin@test.com', '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYfQYgkEzPe')
ON CONFLICT (username) DO NOTHING;
