-- Dashboard Stats
CREATE TABLE IF NOT EXISTS dashboard_stats (
    id SERIAL PRIMARY KEY,
    metric_name VARCHAR(50) NOT NULL,
    value INTEGER NOT NULL
);

-- Recent Activities
CREATE TABLE IF NOT EXISTS recent_activities (
    id SERIAL PRIMARY KEY,
    description TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- System Alerts
CREATE TABLE IF NOT EXISTS system_alerts (
    id SERIAL PRIMARY KEY,
    message TEXT NOT NULL,
    severity VARCHAR(20) NOT NULL -- 'INFO', 'WARNING', 'CRITICAL'
);

-- Seed Data
INSERT INTO dashboard_stats (metric_name, value) VALUES 
    ('total_users', 150),
    ('active_sessions', 23),
    ('monthly_revenue', 12500);

INSERT INTO recent_activities (description) VALUES 
    ('Usuario Pedro se ha logueado'),
    ('Nueva compra realizada'),
    ('Actualización de perfil detectada');

INSERT INTO system_alerts (message, severity) VALUES 
    ('Uso de CPU alto', 'WARNING'),
    ('Backup completado exitosamente', 'INFO');
