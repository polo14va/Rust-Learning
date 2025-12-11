# Deploy Directory

Esta carpeta contiene todos los archivos necesarios para desplegar la API Rust en diferentes entornos.

## 📁 Estructura

```
deploy/
├── README.md                 # Este archivo
├── Dockerfile                # Imagen Docker optimizada para producción
├── DEPLOYMENT.md             # Guía completa de deployment (Fly.io, Railway, etc.)
├── fly.toml                  # Configuración para Fly.io
├── docker/
│   └── docker-compose.yml    # Docker Compose para desarrollo local
└── k8s/
    ├── README.md             # Guía de deployment en Kubernetes
    ├── 00-namespace.yaml     # Namespace
    ├── 01-secrets.yaml       # Secrets (JWT, passwords)
    ├── 02-configmap.yaml     # Configuración
    ├── 03-postgres.yaml      # Base de datos
    ├── 04-redis.yaml         # Cache
    ├── 05-api-deployment.yaml # API (3 réplicas + auto-scaling)
    └── 06-ingress.yaml       # HTTPS externo
```

## 🚀 Opciones de Deployment

### 1️⃣ Desarrollo Local (Docker Compose)
```bash
cd deploy/docker
docker-compose up
```

### 2️⃣ Producción Simple (Fly.io)
```bash
cd deploy
fly launch --config fly.toml
fly deploy
```

### 3️⃣ Producción Enterprise (Kubernetes)
```bash
cd deploy/k8s
kubectl apply -f .
```

## 📖 Guías Detalladas

- **Fly.io / Railway / VPS**: Ver `DEPLOYMENT.md`
- **Kubernetes**: Ver `k8s/README.md`
- **Docker local**: Ver `docker/docker-compose.yml`

## 🔐 Antes de Desplegar

1. **Cambiar secrets** en `k8s/01-secrets.yaml` o usar:
   ```bash
   kubectl create secret generic rust-api-secrets \
     --from-literal=jwt-secret=$(openssl rand -base64 32) \
     --from-literal=postgres-password=$(openssl rand -base64 16)
   ```

2. **Actualizar imagen** en `k8s/05-api-deployment.yaml`:
   ```yaml
   image: tu-registry/rust-api:latest
   ```

3. **Build y push** imagen:
   ```bash
   docker build -t tu-registry/rust-api:latest -f deploy/Dockerfile .
   docker push tu-registry/rust-api:latest
   ```
