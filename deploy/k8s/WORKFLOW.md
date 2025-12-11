# 🔄 Workflow de Desarrollo en Kubernetes

## 📝 Flujo Completo: Cambio de Código → Deploy

### Opción 1: Rebuild Rápido (RECOMENDADO)

```bash
# 1. Hacer cambios en el código (src/...)

# 2. Rebuild imagen con nuevo tag
docker build -t rust-api:v2 -f deploy/Dockerfile .

# 3. Actualizar deployment para usar nueva imagen
kubectl set image deployment/rust-api api=rust-api:v2 -n rust-api

# 4. Ver el rolling update en acción
kubectl rollout status deployment/rust-api -n rust-api

# 5. Ver logs de los nuevos pods
kubectl logs -l app=rust-api -n rust-api -f --tail=50
```

**Ventajas:**
- ✅ Zero-downtime (rolling update)
- ✅ Puedes hacer rollback fácilmente
- ✅ Versionado claro (v1, v2, v3...)

---

### Opción 2: Rebuild con Mismo Tag (Más Simple)

```bash
# 1. Hacer cambios en el código

# 2. Rebuild imagen (mismo tag)
docker build -t rust-api:local -f deploy/Dockerfile .

# 3. Forzar restart de pods (para que usen la nueva imagen)
kubectl rollout restart deployment/rust-api -n rust-api

# 4. Ver progreso
kubectl rollout status deployment/rust-api -n rust-api
```

**Ventajas:**
- ✅ Más simple (no cambias manifiestos)
- ✅ Bueno para desarrollo rápido

**Desventajas:**
- ❌ No puedes hacer rollback fácil
- ❌ Menos control de versiones

---

### Opción 3: Script Automatizado (LO MÁS RÁPIDO)

Crea este script `deploy/k8s/redeploy.sh`:

```bash
#!/bin/bash
set -e

echo "🔨 Rebuilding imagen..."
docker build -t rust-api:$(date +%s) -f deploy/Dockerfile .

echo "🚀 Updating deployment..."
kubectl set image deployment/rust-api api=rust-api:$(date +%s) -n rust-api

echo "⏳ Waiting for rollout..."
kubectl rollout status deployment/rust-api -n rust-api

echo "✅ Deploy completado!"
kubectl get pods -l app=rust-api -n rust-api
```

Luego solo ejecuta:
```bash
./deploy/k8s/redeploy.sh
```

---

## 🔍 Verificar Cambios

### Ver logs en tiempo real
```bash
kubectl logs -l app=rust-api -n rust-api -f
```

### Ver qué versión está corriendo
```bash
kubectl describe deployment rust-api -n rust-api | grep Image
```

### Ver historial de deployments
```bash
kubectl rollout history deployment/rust-api -n rust-api
```

---

## ⏪ Rollback (si algo falla)

### Volver a la versión anterior
```bash
kubectl rollout undo deployment/rust-api -n rust-api
```

### Volver a una versión específica
```bash
# Ver historial
kubectl rollout history deployment/rust-api -n rust-api

# Rollback a revisión específica
kubectl rollout undo deployment/rust-api --to-revision=2 -n rust-api
```

---

## 🐛 Debugging

### Pod no arranca (CrashLoopBackOff)
```bash
# Ver logs del pod que falla
kubectl logs <pod-name> -n rust-api

# Ver logs del pod anterior (antes del crash)
kubectl logs <pod-name> -n rust-api --previous

# Describir el pod (ver eventos)
kubectl describe pod <pod-name> -n rust-api
```

### Entrar a un pod para debugging
```bash
kubectl exec -it <pod-name> -n rust-api -- /bin/sh

# Dentro del pod:
env | grep DATABASE  # Ver variables de entorno
curl localhost:3000/health  # Probar API internamente
```

### Ver eventos del cluster
```bash
kubectl get events -n rust-api --sort-by='.lastTimestamp'
```

---

## 📊 Monitoreo

### Ver uso de recursos
```bash
# CPU y memoria de los pods
kubectl top pods -n rust-api

# CPU y memoria de los nodos
kubectl top nodes
```

### Ver estado del auto-scaling
```bash
kubectl get hpa -n rust-api -w
```

### Ver métricas de un pod específico
```bash
kubectl top pod <pod-name> -n rust-api
```

---

## 🎯 Workflow Recomendado para Producción

```bash
# 1. Desarrollo local (fuera de K8s)
cargo run --release

# 2. Test en K8s local
docker build -t rust-api:test -f deploy/Dockerfile .
kubectl set image deployment/rust-api api=rust-api:test -n rust-api

# 3. Si funciona, tag como versión
docker tag rust-api:test rust-api:v1.2.3
docker push tu-registry/rust-api:v1.2.3  # Si usas registry

# 4. Deploy a producción
kubectl set image deployment/rust-api api=tu-registry/rust-api:v1.2.3 -n rust-api
```

---

## 🔧 Tips Avanzados

### Build más rápido (cachear dependencias)
```bash
# El Dockerfile ya está optimizado para esto
# Solo recompila tu código si no cambias Cargo.toml
```

### Ver diferencias entre versiones
```bash
kubectl diff -f deploy/k8s/05-api-deployment.yaml
```

### Escalar temporalmente para testing
```bash
# Escalar a 1 pod (más fácil de debuggear)
kubectl scale deployment rust-api --replicas=1 -n rust-api

# Volver a 3
kubectl scale deployment rust-api --replicas=3 -n rust-api
```

### Pausar auto-scaling durante debugging
```bash
kubectl delete hpa rust-api-hpa -n rust-api

# Recrear después
kubectl apply -f deploy/k8s/05-api-deployment.yaml
```

---

## 📋 Checklist Pre-Deploy

- [ ] Código compila localmente: `cargo build --release`
- [ ] Tests pasan: `cargo test`
- [ ] Imagen se construye: `docker build -t rust-api:test -f deploy/Dockerfile .`
- [ ] Variables de entorno correctas en ConfigMap/Secrets
- [ ] Migraciones de DB compatibles (no rompen datos existentes)
- [ ] Logs no tienen errores críticos después del deploy
