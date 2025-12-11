# ============================================================================
# DOCKERFILE OPTIMIZADO PARA PRODUCCIÓN
# ============================================================================
# 
# CARACTERÍSTICAS:
# - Multi-stage build (imagen final ~50MB vs 1.5GB)
# - Cachea dependencias para builds rápidos
# - Usuario no-root (seguridad)
# - Solo incluye binario + dependencias runtime
#
# ============================================================================

# ============================================================================
# STAGE 1: Builder (Compila el código Rust)
# ============================================================================
# Usamos nightly porque algunas dependencias requieren edition2024
FROM rustlang/rust:nightly-slim as builder

WORKDIR /app

# Instalar dependencias de compilación
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# TRUCO: Copiar solo Cargo.toml primero para cachear dependencias
# Si solo cambias src/, Docker reutiliza esta capa
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Ahora copiar código real y recompilar (solo tu código, deps ya cacheadas)
COPY . .
RUN cargo build --release

# ----------------------------------------------------------------------------
# STAGE 2: Runtime (Imagen final pequeña)
# ----------------------------------------------------------------------------
FROM debian:bookworm-slim

# Instalar SOLO dependencias runtime (no compiladores)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copiar binario compilado desde builder
COPY --from=builder /app/target/release/hello_world /usr/local/bin/app

# Copiar migraciones (necesarias para sqlx::migrate!)
COPY --from=builder /app/migrations /migrations

# Crear usuario no-root (SEGURIDAD)
RUN useradd -m -u 1000 appuser && \
    chown -R appuser:appuser /migrations
USER appuser

# Puerto (debe coincidir con tu código)
EXPOSE 3000

# Healthcheck (Docker reinicia si falla)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Comando de inicio
CMD ["app"]
