# Deployment Local en Kubernetes (Docker Desktop)

## 🎯 Objetivo
Compilar la imagen Docker desde el DevContainer y deployarla en el cluster local de Kubernetes (Docker Desktop).

## 📋 Pre-requisitos

1. **Docker Desktop con Kubernetes habilitado**
   - Settings → Kubernetes → Enable Kubernetes
   - Esperar a que el cluster esté running

2. **kubectl configurado**
   ```bash
   # Verificar que apunta a docker-desktop
   kubectl config current-context
   # Debe mostrar: docker-desktop
   ```

## 🚀 Deployment Paso a Paso

### 1. Build de la Imagen (desde DevContainer o Mac)

```bash
# Opción A: Desde tu Mac (fuera del DevContainer)
cd /Users/pedro/Documents/Aproyectos/RUST
docker build -t rust-api:local -f deploy/Dockerfile .

# Opción B: Desde el DevContainer
# (Docker Desktop comparte el daemon, así que la imagen estará disponible)
docker build -t rust-api:local -f deploy/Dockerfile .
```

**IMPORTANTE**: Como Docker Desktop comparte el daemon entre el host y los contenedores, la imagen `rust-api:local` estará disponible automáticamente para Kubernetes.

### 2. Verificar que la Imagen Existe

```bash
docker images | grep rust-api
# Deberías ver: rust-api  local  ...
```

### 3. Actualizar Manifiestos para Uso Local

```bash
# Editar deploy/k8s/05-api-deployment.yaml
# Cambiar línea 25:
# DE:   image: tu-registry/rust-api:latest
# A:    image: rust-api:local
#       imagePullPolicy: Never  # IMPORTANTE: No intentar pull de registry
```

O usar este comando para hacerlo automáticamente:
```bash
sed -i '' 's|image: tu-registry/rust-api:latest|image: rust-api:local|g' deploy/k8s/05-api-deployment.yaml
sed -i '' 's|imagePullPolicy: Always|imagePullPolicy: Never|g' deploy/k8s/05-api-deployment.yaml
```

### 4. Crear Secrets (Local)

```bash
kubectl create secret generic rust-api-secrets \
  --from-literal=jwt-secret=local_dev_secret_key_change_in_prod \
  --from-literal=postgres-password=postgres \
  --namespace=rust-api \
  --dry-run=client -o yaml | kubectl apply -f -
```

### 5. Deploy Completo

```bash
# Aplicar todos los manifiestos
kubectl apply -f deploy/k8s/

# Ver el progreso
kubectl get all -n rust-api -w
```

### 6. Esperar a que Todo Esté Listo

```bash
# Esperar Postgres
kubectl wait --for=condition=ready pod -l app=postgres -n rust-api --timeout=120s

# Esperar Redis
kubectl wait --for=condition=ready pod -l app=redis -n rust-api --timeout=60s

# Esperar API (3 pods)
kubectl wait --for=condition=ready pod -l app=rust-api -n rust-api --timeout=180s
```

### 7. Acceder a la API

```bash
# Port-forward del servicio
kubectl port-forward svc/rust-api-service 8080:80 -n rust-api

# En otra terminal, probar:
curl http://localhost:8080/health
curl http://localhost:8080/users
```

## 🔄 Workflow de Desarrollo

### Hacer Cambios y Re-deployar

```bash
# 1. Hacer cambios en el código

# 2. Rebuild imagen
docker build -t rust-api:local -f deploy/Dockerfile .

# 3. Forzar recreación de pods (para que usen la nueva imagen)
kubectl rollout restart deployment/rust-api -n rust-api

# 4. Ver progreso
kubectl rollout status deployment/rust-api -n rust-api

# 5. Ver logs
kubectl logs -l app=rust-api -n rust-api -f
```

## 🐛 Debugging

### Ver logs de un pod específico
```bash
kubectl get pods -n rust-api
kubectl logs <pod-name> -n rust-api -f
```

### Entrar a un pod
```bash
kubectl exec -it <pod-name> -n rust-api -- /bin/sh
```

### Ver eventos (errores)
```bash
kubectl get events -n rust-api --sort-by='.lastTimestamp'
```

### Describir un pod (ver por qué falla)
```bash
kubectl describe pod <pod-name> -n rust-api
```

### Ver estado de Postgres
```bash
kubectl exec -it postgres-0 -n rust-api -- psql -U postgres -d rust_db -c '\dt'
```

### Ver estado de Redis
```bash
kubectl exec -it <redis-pod> -n rust-api -- redis-cli ping
```

## 🧹 Limpieza

### Eliminar todo
```bash
kubectl delete namespace rust-api
```

### Eliminar solo la API (mantener BBDD)
```bash
kubectl delete deployment rust-api -n rust-api
kubectl delete hpa rust-api-hpa -n rust-api
```

## 📊 Monitoreo

### Dashboard de Kubernetes (opcional)
```bash
# Instalar dashboard
kubectl apply -f https://raw.githubusercontent.com/kubernetes/dashboard/v2.7.0/aio/deploy/recommended.yaml

# Crear usuario admin
kubectl create serviceaccount dashboard-admin -n kubernetes-dashboard
kubectl create clusterrolebinding dashboard-admin --clusterrole=cluster-admin --serviceaccount=kubernetes-dashboard:dashboard-admin

# Obtener token
kubectl create token dashboard-admin -n kubernetes-dashboard

# Acceder
kubectl proxy
# Abrir: http://localhost:8001/api/v1/namespaces/kubernetes-dashboard/services/https:kubernetes-dashboard:/proxy/
```

### Ver métricas de recursos
```bash
# CPU y memoria de los pods
kubectl top pods -n rust-api

# CPU y memoria de los nodos
kubectl top nodes
```

## 🎓 Tips

1. **Usar `imagePullPolicy: Never`** en local para que no intente descargar de registry
2. **Usar tags únicos** (ej: `rust-api:v1`, `rust-api:v2`) para evitar confusión
3. **`kubectl rollout restart`** es más rápido que delete + apply
4. **Port-forward** es perfecto para desarrollo local
5. **LoadBalancer** en Docker Desktop usa `localhost` automáticamente

## 🚨 Problemas Comunes

### "ImagePullBackOff"
- Verificar que `imagePullPolicy: Never`
- Verificar que la imagen existe: `docker images | grep rust-api`

### "CrashLoopBackOff"
- Ver logs: `kubectl logs <pod> -n rust-api`
- Verificar que Postgres y Redis están listos
- Verificar variables de entorno en el deployment

### Postgres no conecta
- Verificar que el service existe: `kubectl get svc -n rust-api`
- Verificar DNS: `kubectl exec -it <api-pod> -n rust-api -- nslookup postgres-service`

### Migraciones fallan
- Verificar que el pod tiene acceso a `/migrations`
- Ver logs del pod durante startup
