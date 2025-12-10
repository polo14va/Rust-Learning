-- Create Users Table
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    username VARCHAR(50) NOT NULL UNIQUE,
    email VARCHAR(100) NOT NULL
);

-- Seed Data (Insertar datos de prueba si no existen)
INSERT INTO users (username, email) 
VALUES 
    ('pedro_senior', 'pedro@example.com'),
    ('rust_fan', 'rustacean@example.org')
ON CONFLICT (username) DO NOTHING;
