-- Allow web-demo callback on port 8000 and expand scopes
UPDATE oauth_clients
SET redirect_uris = 'http://localhost:3000/callback,http://127.0.0.1:3000/callback,http://localhost:8000/callback.html,http://127.0.0.1:8000/callback.html',
    scopes = 'openid profile email offline_access dashboard.read'
WHERE client_id = 'demo-client';
