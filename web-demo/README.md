# Web Demo (HTML/CSS/JS)

Demo basico para probar el 100% del servicio de auth.

## Como levantarlo

1) Levanta la API y dependencias:
```bash
cargo run
```

2) Opcion recomendada: servir desde la propia API:
```
http://localhost:3000/demo/
```

3) Alternativa: servir esta carpeta como estatico (Python):
```bash
cd web-demo
python3 -m http.server 8000
```

4) Abre en el navegador:
```
http://localhost:8000
```

## Nota sobre CORS y cookies SSO

La API no expone CORS por defecto. Para usar esta web desde otro puerto:
- O bien habilita CORS en la API.
- O bien sirve la web en el mismo origen mediante un reverse proxy.

El login SSO usa una cookie. Si sirves la web en otro puerto, el navegador puede
bloquear la cookie por ser un contexto third-party. La opcion mas estable es:

```
http://localhost:3000/demo/
```

## OAuth Code Flow

Para que el authorize/consent funcione, el `redirect_uri` debe existir en
`oauth_clients.redirect_uris`. Por defecto solo estan permitidos:
- `http://localhost:3000/callback`
- `http://127.0.0.1:3000/callback`

Si usas este demo en `http://localhost:8000/callback.html`, agrega esa URL
al cliente `demo-client` en la DB o crea un nuevo cliente con esa redirect URI.

## Login SSO en modal (modo token)

El boton "Login SSO" carga `/login?mode=token` en un iframe. En este modo,
el login envia credenciales a `/login` (JSON) y devuelve tokens via
`postMessage` para guardarlos en `localStorage`. No depende de cookies.

## Datos de prueba

- Usuario: `admin`
- Password: `test123`
- Client ID: `demo-client`
- Client Secret: `demo-secret`
