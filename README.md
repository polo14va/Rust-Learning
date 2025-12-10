# Rust Learning: API REST con Axum & SQLx (DevContainer Ready)

Este repositorio es un **boilerplate educativo** diseñado para aprender Rust, específicamente el desarrollo de APIs REST, sin dolor de cabeza en la configuración del entorno.

El proyecto implementa una API básica para gestionar usuarios, conectada a una base de datos PostgreSQL, todo orquestado mediante Docker.

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

### Endpoints Disponibles
*   `GET /`: Health check simple.
*   `GET /users`: Lista los usuarios desde la base de datos Postgres.

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
