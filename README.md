# Rust Learning: API REST con Axum & SQLx (DevContainer Ready)

Este repositorio es un **boilerplate educativo** diseñado para aprender Rust, específicamente el desarrollo de APIs REST, sin dolor de cabeza en la configuración del entorno.

El proyecto implementa una API básica para gestionar usuarios, conectada a una base de datos PostgreSQL, todo orquestado mediante Docker. Evolucionará hacia un **Authorization Server OAuth2.0/OIDC + SSO corporativo** en Rust, con despliegue en Kubernetes y capacidad de autenticar todas las apps internas en tiempo real.

## 🚀 ¿Qué es un DevContainer?

Si eres nuevo en Rust (o en Docker), este proyecto usa una tecnología llamada **DevContainer** (Development Container).

**¿El problema estándar?**
Normalmente, para programar en Rust necesitas instalar `rustup`, `cargo`, dependencias de sistema, configurar tu IDE, instalar PostgreSQL en tu Mac/Windows, lidiar con versiones, etc. "En mi máquina funciona" es el clásico problema.

**¿La solución DevContainer?**
Todo el entorno de desarrollo (compilador, herramientas, extensiones de VS Code, debugger) vive dentro de un contenedor Docker Linux.
*   **Aislamiento**: No ensucias tu sistema operativo principal.
*   **Reproducibilidad**: Cualquier persona que clone este repo tendrá **exactamente** el mismo entorno que tú.
*   **Comodidad**: VS Code se conecta al contenedor y se siente como si programaras en local, pero estás dentro de Linux.

## 🛠 Tech Stack

*   **Lenguaje**: Rust (Edition 2021)
*   **Web Framework**: [Axum](https://github.com/tokio-rs/axum) (Ergonómico y modular)
*   **Async Runtime**: [Tokio](https://tokio.rs/)
*   **Base de Datos**: PostgreSQL 15 (vía Docker Compose)
*   **SQL Driver**: [SQLx](https://github.com/launchbadge/sqlx) (Validación de queries en tiempo de compilación)
*   **Arquitectura**: Capas (Handlers, Models, DB Repository, Errors).

## 🚦 Evolución a Auth 2.0 + SSO corporativo

Se transformará en un servidor de autorización/OIDC de alto rendimiento:

- Protocolos: OAuth2 Authorization Code + PKCE, Client Credentials, Refresh Token, revocación e introspección; OpenID Connect (`/.well-known/openid-configuration`, `jwks.json`, `userinfo`).
- Tokens y claves: JWT RS256/ES256 con rotación y `kid`, endpoint JWKS, cache en Redis, expiraciones configurables; hashing de passwords con Argon2id.
- SSO y sesiones: cookie segura de sesión, login único para todas las apps, logout global, flujo de consentimiento por scopes, validación estricta de `redirect_uri` y `state/nonce`.
- Gestión de clientes/usuarios: alta/rotación de secretos, scopes permitidos, CRUD de usuarios y roles, auditoría de eventos y revocación de sesiones/refresh tokens.
- Seguridad: rate limiting, bloqueo temporal por fuerza bruta, políticas de contraseña, headers seguros y CSP para formularios.
- Observabilidad: métricas Prometheus (emisión/validación de tokens, fallos), tracing estructurado, health/readiness.
- K8s-ready: contenedor slim, Deployment/Service/Ingress TLS, ConfigMap/Secrets para claves, Job de migraciones, probes y HPA.
- Integración: ejemplo de Resource Server Axum que valida tokens vía JWKS cache, snippets para apps internas (backend y frontend).

Hoja de ruta resumida (alto nivel):
1) Modelado y migraciones: usuarios, clientes OAuth, scopes/roles, códigos de autorización/PKCE, tokens/refresh, claves JWK, sesiones SSO, auditoría.  
2) Endpoints OAuth2/OIDC y UI mínima de login/consentimiento.  
3) Middleware/SDK para validación en apps internas y sample resource server.  
4) Hardening, métricas, tests de integración y carga.  
5) Manifests de Kubernetes y guía de despliegue corporativo.

## 🏁 Cómo empezar (Quickstart)

### Requisitos previos
1.  [Docker Desktop](https://www.docker.com/products/docker-desktop/) instalado y corriendo.
2.  [VS Code](https://code.visualstudio.com/) instalado.
3.  Extensión [Dev Containers](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers) para VS Code.

### Pasos
1.  Clona este repo.
2.  Abre la carpeta en VS Code.
3.  Verás una notificación abajo a la derecha: *"Reopen in Container"* (O busca en la paleta de comandos `Dev Containers: Reopen in Container`).
4.  Espera unos minutos a que Docker construya la imagen.
5.  Una vez dentro, abre una terminal (que será una terminal de Linux) y ejecuta:

```bash
cargo run
```

¡Listo! El servidor estará escuchando en `http://localhost:3000`.

### Endpoints actuales (fase boilerplate)
*   `GET /`: Health check simple.
*   `GET /health`: Estado de servicio.
*   `GET /metrics`: Métricas Prometheus.
*   `GET /users`: Lista los usuarios desde la base de datos Postgres.
*   `GET /login`: UI de login SSO (cookie + sesión en Redis).
*   `POST /register`: Alta de usuario (dev).
*   `POST /login`: Login con JWT + refresh.
*   `POST /refresh`: Renovación de access token.
*   `POST /logout`: Revoca refresh token.
*   `GET /dashboard`: Endpoint protegido con middleware de autenticación.

### Endpoints previstos (fase Auth 2.0/OIDC)
*   `/authorize` (Authorization Code + PKCE)  
*   `/token` (code exchange, client credentials, refresh)  
*   `/introspect`, `/revoke`  
*   `/userinfo`, `/.well-known/openid-configuration`, `/.well-known/jwks.json`  
*   `/login` (GET UI / POST API), `/consent` (UI de scopes), `/logout` (SSO)  
*   APIs de administración de clientes, scopes y usuarios.

### Variables de entorno clave (Auth/SSO)
- `OIDC_ISSUER` (default `http://localhost:3000`)
- `JWT_PRIVATE_KEY_PEM` / `JWT_PUBLIC_KEY_PEM` (RSA). Si faltan, se generan claves efímeras para desarrollo.
- `SESSION_TTL_MINUTES` (default `60`)
- `REFRESH_TOKEN_TTL_DAYS` (default `7`)
- `RATE_LIMIT_PER_SECOND` (default `10`)
- `DATABASE_URL`, `REDIS_URL`

## 📂 Estructura del Proyecto

*   `.devcontainer/`: Configuración para que VS Code sepa cómo crear el entorno.
*   `docker-compose.yml`: Define la base de datos PostgreSQL.
*   `migrations/`: Scripts SQL para crear tablas.
*   `src/`:
    *   `main.rs`: Punto de entrada y configuración.
    *   `models.rs`: Estructuras de datos (Structs).
    *   `db.rs`: Capa de acceso a datos (Queries).
    *   `handlers.rs`: Controladores HTTP.
    *   `error.rs`: Manejo de errores centralizado.

## 🐛 Debugging
El DevContainer ya viene pre-configurado con `lldb`. Puedes poner breakpoints en VS Code y presionar F5 para depurar tu código Rust paso a paso.
