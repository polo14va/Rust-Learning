-- Add role to users
ALTER TABLE users ADD COLUMN IF NOT EXISTS role VARCHAR(32) NOT NULL DEFAULT 'user';

-- Set admin role for admin seed user
UPDATE users SET role = 'admin' WHERE username = 'admin';
