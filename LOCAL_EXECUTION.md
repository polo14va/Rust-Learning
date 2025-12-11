# Guía de Ejecución Local Óptima

## 🎯 Opción 1: Máximo Rendimiento (RECOMENDADO)

### Setup (solo una vez)
```bash
# 1. Levantar solo Postgres y Redis en Docker
docker-compose up -d db redis

# 2. Compilar en modo release
cargo build --release
```

### Ejecución diaria
```bash
# Ejecutar binario optimizado
./target/release/hello_world
```

**Rendimiento:**
- Startup: ~100ms
- Latencia: ~0.8ms por request
- Memoria: ~15MB

---

## 🔄 Opción 2: Desarrollo con Hot Reload

### Setup
```bash
# Instalar cargo-watch
cargo install cargo-watch

# Levantar dependencias
docker-compose up -d db redis
```

### Ejecución
```bash
# Auto-recompila en cada cambio de código
cargo watch -x 'run --release'
```

**Ventajas:**
- Cambias código → Auto-reinicia
- Rendimiento casi igual a binario

---

## 🐳 Opción 3: Docker Completo (Testing Pre-Producción)

### docker-compose.yml optimizado
```yaml
version: '3.8'

services:
  # Postgres
  db:
    image: postgres:15-alpine
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: rust_db
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data

  # Redis
  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"

  # API Rust (OPTIMIZADA)
  api:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "3000:3000"
    environment:
      DATABASE_URL: postgres://postgres:postgres@db:5432/rust_db
      REDIS_URL: redis://redis/
      JWT_SECRET: dev_secret_key
      JWT_EXPIRATION_MINUTES: 15
      RATE_LIMIT_PER_SECOND: 10
      RUST_LOG: info
    depends_on:
      - db
      - redis

volumes:
  postgres_data:
```

### Ejecución
```bash
# Build + Run todo
docker-compose up --build

# Solo rebuild API (si cambias código)
docker-compose up --build api
```

---

## ⚡ Optimizaciones Adicionales

### 1. Compilación Incremental (más rápida)
```bash
# En .cargo/config.toml
[build]
incremental = true
```

### 2. Linker más rápido (macOS)
```bash
# Instalar mold (linker ultra-rápido)
brew install mold

# En .cargo/config.toml
[target.aarch64-apple-darwin]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=/opt/homebrew/bin/mold"]
```

**Resultado:** Compilación 2-3x más rápida

### 3. Profile optimizado
```toml
# En Cargo.toml
[profile.release]
opt-level = 3          # Máxima optimización
lto = "thin"           # Link-Time Optimization
codegen-units = 1      # Mejor optimización (más lento compilar)
strip = true           # Quitar símbolos debug (binario más pequeño)
```

---

## 📊 Comparación Final

| Método | Startup | Hot Reload | Rendimiento | Realismo Prod |
|--------|---------|------------|-------------|---------------|
| **Binario nativo** | ⭐⭐⭐ | ❌ | ⭐⭐⭐ | ⭐⭐ |
| **cargo watch** | ⭐⭐ | ✅ | ⭐⭐⭐ | ⭐⭐ |
| **Docker** | ⭐ | ❌ | ⭐⭐ | ⭐⭐⭐ |

---

## 🎓 Mi Recomendación

**Para ti (desarrollo en Mac):**

```bash
# Terminal 1: Dependencias
docker-compose up db redis

# Terminal 2: API con hot reload
cargo install cargo-watch
cargo watch -x 'run --release'
```

**Ventajas:**
- ✅ Cambias código → Auto-reinicia
- ✅ Máximo rendimiento (nativo)
- ✅ Postgres/Redis aislados
- ✅ No ensucias tu Mac con dependencias

**Antes de producción:**
```bash
# Test con Docker completo
docker-compose up --build
```
