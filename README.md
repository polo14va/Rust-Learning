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

### Endpoints actuales
*   `GET /`: Respuesta simple (root).
*   `GET /health`: Estado del servicio (Postgres + Redis).
*   `GET /metrics`: Métricas Prometheus.
*   `GET /users`: Lista usuarios desde Postgres.
*   `GET /dashboard`: Endpoint protegido con middleware de autenticación.
*   `GET /login`: UI de login SSO.
*   `POST /login`: Login con JWT + refresh.
*   `POST /register`: Alta de usuario (dev).
*   `POST /refresh`: Renovación de access token.
*   `POST /logout`: Revoca refresh token + sesión SSO.

### Endpoints OAuth2/OIDC implementados
*   `GET /authorize`: Authorization Code + PKCE.
*   `POST /token`: code exchange, refresh, client_credentials.
*   `POST /introspect`: Introspección de token.
*   `POST /revoke`: Revocación de refresh token.
*   `GET /userinfo`: Datos de usuario autenticado.
*   `GET /.well-known/openid-configuration`: Descubrimiento OIDC.
*   `GET /.well-known/jwks.json`: JWKS público.
*   `GET /consent`: UI de consentimiento.
*   `POST /consent`: Envío de consentimiento.

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

## 🧠 Funcionamiento interno (resumen exhaustivo)

### Arranque de la aplicación
1. Carga variables de entorno con `dotenvy` y configura logging con `tracing`.
2. Conecta a Postgres con `sqlx` y ejecuta migraciones.
3. Registra un cliente OAuth interno "first-party" si no existe.
4. Conecta a Redis para sesiones, refresh tokens y cache.
5. Carga claves JWT RSA desde `JWT_PRIVATE_KEY_PEM` o genera una efímera para desarrollo.
6. Levanta el servidor Axum en `0.0.0.0:3000`.

### Autenticación, SSO y tokens
- Passwords con Argon2, con compatibilidad para hashes Bcrypt antiguos.
- JWT RS256 con `kid` derivado de la clave pública.
- Refresh tokens guardados en Redis y auditados en Postgres.
- Sesiones SSO con cookie `sso_session` y TTL configurable.
- Rate limiting por username en Redis (ventana de 60s).
- PKCE con `S256` cuando corresponde.
- Consentimiento por scopes almacenado en Redis con TTL.

### Cache y dashboard protegido
- `/dashboard` está protegido por middleware que valida Bearer JWT.
- Se cachea en Redis con TTL 60s para reducir carga.
- Las consultas a DB se ejecutan concurrentemente (`tokio::join!`).

### Observabilidad
- Middleware registra métricas HTTP: total, latencia, status.
- Métricas de cache, auth y rate limiting expuestas en `/metrics`.
- Health check validando Postgres y Redis en `/health`.

## 🏗 Arquitectura (alto nivel)

```
Cliente/Browser
   |
   v
Axum API (Rust)  --->  Redis (sesiones, refresh, consent, cache)
   |
   v
Postgres (usuarios, clientes OAuth, auth codes, refresh tokens)
```

## 🔄 Flujos clave (paso a paso)

### 1) Login SSO (UI)
1. Usuario abre `GET /login`.
2. Envía credenciales a `POST /login/form`.
3. Se valida password y se crea una sesión SSO en Redis.
4. Se devuelve cookie `sso_session`.

### 2) OAuth2 Authorization Code + PKCE
1. Cliente llama `GET /authorize` con `client_id`, `redirect_uri`, `scope`, `code_challenge`.
2. Si no hay sesión SSO, redirige a `GET /login`.
3. Si no hay consentimiento previo, muestra `GET /consent`.
4. Al aprobar, genera código y redirige al `redirect_uri`.
5. Cliente intercambia código en `POST /token` con `code_verifier`.

### 3) Refresh Token
1. Cliente envía `POST /token` con `grant_type=refresh_token`.
2. Se valida en Redis y en Postgres (revocación/expiración).
3. Se emite nuevo access token.

### 4) Acceso a recursos protegidos
1. Cliente envía `Authorization: Bearer <access_token>`.
2. Middleware valida JWT y permite acceso (ej: `GET /dashboard`).

## 🔐 Seguridad y controles

### Passwords
- Argon2 como algoritmo principal, compatibilidad con hashes Bcrypt existentes.

### Tokens
- JWT RS256 con `kid` y JWKS público.
- Refresh tokens con TTL y revocación persistida.

### Sesiones y consentimiento
- Sesiones SSO en Redis con TTL configurable.
- Consentimiento almacenado por `usuario + cliente + scope` con expiración.

### Rate limiting
- Implementado en Redis con ventana de 60s.
- Clave basada en username por endpoint crítico (`login`, `register`).

## 🧪 Datos de ejemplo
- Usuario `admin` creado en migraciones con password `test123`.
- Cliente OAuth `demo-client` con `demo-secret` y redirect URIs locales.

## ⚙️ Variables de entorno (detalladas)

Obligatorias:
- `DATABASE_URL`
- `REDIS_URL`

Opcionales (con defaults):
- `OIDC_ISSUER` (default `http://localhost:3000`)
- `JWT_PRIVATE_KEY_PEM` (si no existe, se genera clave efímera)
- `SESSION_TTL_MINUTES` (default `60`)
- `REFRESH_TOKEN_TTL_DAYS` (default `7`)
- `RATE_LIMIT_PER_SECOND` (default `10`)

## 🗃 Modelo de datos (Postgres)
- `users`: usuarios con `password_hash`.
- `dashboard_stats`, `recent_activities`, `system_alerts`: datos del dashboard.
- `oauth_clients`: clientes OAuth registrados.
- `oauth_authorization_codes`: códigos de autorización (PKCE).
- `oauth_refresh_tokens`: refresh tokens con expiración y revocación.
- `oauth_jwks`: metadatos de claves (listo para rotación).

## 🔌 Puertos expuestos

### Aplicación
- `3000/tcp`: API Axum (en local y contenedores).
- Fly.io: expone `80` y `443` hacia el puerto interno `3000`.

### Docker Compose (local)
- Postgres: `5432`.
- Redis: `6379`.
- API: `3000` (si se habilita el servicio).

### Kubernetes (manifests en `deploy/k8s`)
- API: `3000` (container), Service `80 -> 3000`.
- Postgres: `5432` (NodePort `30432`).
- Redis: `6379` (NodePort `30379`).
- Prometheus: `9090` (NodePort `30900`).
- Grafana: `3000` (NodePort `30300`).

## 🚢 Despliegue (resumen)

### Docker
- Imagen multi-stage en `deploy/Dockerfile`.
- Healthcheck HTTP en `/health`.

### Kubernetes
- Deployment con 3 réplicas y auto-scaling.
- Liveness/Readiness/Startup probes.
- ConfigMap + Secrets para configuración sensible.
- Ingress opcional con TLS.

### Fly.io
- Configurado en `deploy/fly.toml` con puertos 80/443.

## 🐛 Debugging
El DevContainer ya viene pre-configurado con `lldb`. Puedes poner breakpoints en VS Code y presionar F5 para depurar tu código Rust paso a paso.
