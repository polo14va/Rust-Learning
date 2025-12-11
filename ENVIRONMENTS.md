# Guía de Configuración Multi-Entorno

Tu código ahora es **agnóstico del entorno**. Lee todas las configuraciones de variables de entorno.

## 🎯 Configuración por Entorno

### 1️⃣ Desarrollo Local (cargo run)

```bash
# 1. Copia el template
cp .env.example .env

# 2. Edita .env (ya está configurado para host.docker.internal)
# DATABASE_URL=postgres://postgres:postgres@host.docker.internal:5432/rust_db
# REDIS_URL=redis://host.docker.internal:6379/

# 3. Levanta dependencias
docker-compose up -d db redis

# 4. Corre la API
cargo run --release
```

---

### 2️⃣ Docker Compose (todo en contenedores)

```bash
# 1. Levanta todo (API + DB + Redis)
docker-compose up

# Las variables de entorno están en docker-compose.yml:
# DATABASE_URL: postgres://postgres:postgres@db:5432/rust_db
# REDIS_URL: redis://redis:6379/
```

**Ventaja:** No necesitas `.env`, todo está en `docker-compose.yml`

---

### 3️⃣ Kubernetes

```bash
# Las variables se inyectan desde:
# - ConfigMap (deploy/k8s/02-configmap.yaml)
# - Secrets (deploy/k8s/01-secrets.yaml)

# Deploy
./deploy/k8s/deploy-local.sh

# Las URLs en K8s son:
# DATABASE_URL: postgres://postgres:PASSWORD@postgres-service:5432/rust_db
# REDIS_URL: redis://redis-service:6379/
```

**Ventaja:** Configuración centralizada en manifiestos

---

## 📊 Comparación

| Entorno | DATABASE_URL | REDIS_URL | Configuración |
|---------|--------------|-----------|---------------|
| **cargo run** | `host.docker.internal:5432` | `host.docker.internal:6379` | `.env` |
| **Docker Compose** | `db:5432` | `redis:6379` | `docker-compose.yml` |
| **Kubernetes** | `postgres-service:5432` | `redis-service:6379` | ConfigMap/Secrets |

---

## 🔧 Variables Disponibles

```bash
# Obligatorias
DATABASE_URL=postgres://user:pass@host:port/dbname
REDIS_URL=redis://host:port/

# Opcionales (con defaults)
JWT_SECRET=tu_secreto                # Default: fallback_secret_key
JWT_EXPIRATION_MINUTES=15            # Default: 15
RATE_LIMIT_PER_SECOND=10             # Default: 10
RUST_LOG=info                        # Default: (sin logs)
```

---

## 🎓 Ejemplos de Uso

### Cambiar a Postgres local (sin Docker)
```bash
# En .env
DATABASE_URL=postgres://postgres:postgres@localhost:5432/rust_db
REDIS_URL=redis://localhost:6379/
```

### Usar Postgres en la nube
```bash
# En .env
DATABASE_URL=postgres://user:pass@my-db.aws.com:5432/rust_db
REDIS_URL=redis://my-redis.aws.com:6379/
```

### Testing con diferentes configuraciones
```bash
# Sobrescribir env vars temporalmente
DATABASE_URL=postgres://test@localhost/test_db cargo test
```

---

## ✅ Checklist de Migración

- [x] Código lee `DATABASE_URL` de env var
- [x] Código lee `REDIS_URL` de env var
- [x] Código lee `JWT_SECRET` de env var
- [x] `.env.example` creado con valores de ejemplo
- [x] `docker-compose.yml` actualizado con env vars
- [x] Kubernetes ConfigMap/Secrets configurados
- [x] Fallbacks para desarrollo local

---

## 🚨 Importante

**Nunca commitees `.env` a Git** (ya está en `.gitignore`).  
Solo commitea `.env.example` como template.
