# Guía de Deployment a Producción - Rust API

## 🎯 Opciones de Hosting (Ordenadas por Simplicidad)

### 1️⃣ Fly.io (RECOMENDADO para empezar)
**Ventajas:**
- Deploy en 1 comando
- SSL/HTTPS automático
- Postgres y Redis managed incluidos
- Free tier: 3 VMs pequeñas gratis
- Optimizado para Rust

**Pasos:**
```bash
# Instalar CLI
curl -L https://fly.io/install.sh | sh

# Login
fly auth login

# Deploy (primera vez)
fly launch

# Configurar secrets
fly secrets set JWT_SECRET=$(openssl rand -base64 32)
fly secrets set DATABASE_URL=postgres://...
fly secrets set REDIS_URL=redis://...

# Deploy futuro (automático en cada push si conectas GitHub)
fly deploy
```

**Costo estimado:** $0-5/mes (free tier cubre desarrollo)

---

### 2️⃣ Railway.app (MÁS FÁCIL)
**Ventajas:**
- Deploy desde GitHub (push = deploy automático)
- Postgres + Redis con 1 click
- No necesitas Dockerfile
- Dashboard visual

**Pasos:**
1. Ve a railway.app
2. "New Project" → "Deploy from GitHub"
3. Selecciona tu repo
4. Añade Postgres y Redis desde "New" → "Database"
5. Railway auto-detecta Rust y hace build

**Costo estimado:** $5/mes

---

### 3️⃣ Docker + VPS (DigitalOcean/Hetzner)
**Ventajas:**
- Control total
- Más barato a largo plazo
- Aprenderás Docker/Linux

**Pasos:**
```bash
# 1. Crear Dockerfile optimizado (ver abajo)
# 2. Subir a VPS
scp -r . user@tu-vps:/app

# 3. En el VPS
docker-compose up -d
```

**Costo estimado:** $5-10/mes (VPS) + $0 (self-hosted DB)

---

### 4️⃣ AWS ECS / GCP Cloud Run (ENTERPRISE)
**Ventajas:**
- Auto-scaling
- Alta disponibilidad
- Integración con otros servicios cloud

**Costo estimado:** $50-200/mes (depende del tráfico)

---

## 📦 Dockerfile Optimizado para Producción

```dockerfile
# ============================================================================
# STAGE 1: Builder (compila el código)
# ============================================================================
FROM rust:1.75-slim as builder

WORKDIR /app

# Copiar solo Cargo.toml primero (cachea dependencias)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copiar código real y compilar
COPY . .
RUN cargo build --release

# ============================================================================
# STAGE 2: Runtime (imagen final pequeña)
# ============================================================================
FROM debian:bookworm-slim

# Instalar dependencias runtime (SSL, CA certificates)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copiar binario compilado
COPY --from=builder /app/target/release/hello_world /usr/local/bin/app

# Usuario no-root (seguridad)
RUN useradd -m appuser
USER appuser

# Puerto
EXPOSE 3000

# Comando
CMD ["app"]
```

**Resultado:** Imagen de ~50MB (vs 1.5GB sin multi-stage)

---

## 🔒 Checklist de Seguridad Pre-Producción

### ✅ Variables de Entorno
- [ ] `JWT_SECRET` con 32+ caracteres aleatorios
- [ ] `DATABASE_URL` con usuario/password seguros
- [ ] No hardcodear secretos en el código

### ✅ HTTPS/SSL
- [ ] Usar Fly.io/Railway (SSL automático)
- [ ] O configurar Nginx + Let's Encrypt

### ✅ Base de Datos
- [ ] Usar Postgres managed (no self-hosted en producción)
- [ ] Backups automáticos habilitados
- [ ] Conexiones SSL a la DB

### ✅ Logging y Monitoreo
- [ ] Logs estructurados (ya tienes `tracing` ✅)
- [ ] Configurar nivel de logs: `RUST_LOG=info` (no `debug` en prod)
- [ ] Opcional: Integrar con Sentry/Datadog

### ✅ Rate Limiting
- [ ] Ya tienes Redis rate limiting ✅
- [ ] Ajustar límites según tráfico esperado

### ✅ Healthcheck
- [ ] Ya tienes `/health` ✅
- [ ] Configurar en Docker/K8s para auto-restart

---

## 🚀 Comando de Deploy Rápido (Fly.io)

```bash
# 1. Crear fly.toml (solo primera vez)
cat > fly.toml << 'EOF'
app = "tu-api-rust"

[build]
  builder = "paketobuildpacks/builder:base"

[env]
  PORT = "3000"

[[services]]
  internal_port = 3000
  protocol = "tcp"

  [[services.ports]]
    port = 80
    handlers = ["http"]
  
  [[services.ports]]
    port = 443
    handlers = ["tls", "http"]
EOF

# 2. Crear Postgres
fly postgres create

# 3. Conectar Postgres a tu app
fly postgres attach <postgres-app-name>

# 4. Crear Redis (Upstash)
fly redis create

# 5. Deploy
fly deploy

# 6. Ver logs
fly logs
```

---

## 📊 Comparación de Opciones

| Opción | Dificultad | Costo/mes | Auto-scaling | SSL | Managed DB |
|--------|-----------|-----------|--------------|-----|------------|
| Fly.io | ⭐⭐ | $0-5 | ✅ | ✅ | ✅ |
| Railway | ⭐ | $5 | ✅ | ✅ | ✅ |
| VPS + Docker | ⭐⭐⭐ | $5-10 | ❌ | Manual | ❌ |
| AWS ECS | ⭐⭐⭐⭐⭐ | $50+ | ✅ | ✅ | ✅ |

---

## 🎓 Mi Recomendación

**Para aprender/MVP:** Fly.io o Railway
**Para producción seria:** Fly.io con Postgres managed
**Para enterprise:** AWS ECS + RDS

---

## 📝 Próximos Pasos

1. Elige una plataforma (recomiendo Fly.io)
2. Crea el Dockerfile optimizado
3. Configura secrets (JWT_SECRET, DATABASE_URL)
4. Deploy
5. Prueba con `curl https://tu-app.fly.dev/health`
