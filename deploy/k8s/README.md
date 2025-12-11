# Guía de Deployment en Kubernetes

## 📋 Pre-requisitos

1. **Cluster de Kubernetes** (elige uno):
   - Minikube (local): `brew install minikube && minikube start`
   - Docker Desktop (local): Habilitar Kubernetes en settings
   - GKE (Google): `gcloud container clusters create rust-cluster`
   - EKS (AWS): `eksctl create cluster --name rust-cluster`
   - AKS (Azure): `az aks create --name rust-cluster`

2. **kubectl** instalado:
   ```bash
   brew install kubectl
   ```

3. **Imagen Docker** de tu API:
   ```bash
   # Build
   docker build -t tu-registry/rust-api:latest .
   
   # Push a registry (Docker Hub, GCR, ECR, etc.)
   docker push tu-registry/rust-api:latest
   ```

---

## 🚀 Deployment Paso a Paso

### 1. Crear Namespace
```bash
kubectl apply -f k8s/00-namespace.yaml
```

### 2. Crear Secrets (IMPORTANTE: Cambiar valores)
```bash
# Opción A: Desde archivo (NO RECOMENDADO en producción)
kubectl apply -f k8s/01-secrets.yaml

# Opción B: Desde línea de comandos (RECOMENDADO)
kubectl create secret generic rust-api-secrets \
  --from-literal=jwt-secret=$(openssl rand -base64 32) \
  --from-literal=postgres-password=$(openssl rand -base64 16) \
  --namespace=rust-api
```

### 3. Crear ConfigMap
```bash
kubectl apply -f k8s/02-configmap.yaml
```

### 4. Desplegar Postgres
```bash
kubectl apply -f k8s/03-postgres.yaml

# Esperar a que esté listo
kubectl wait --for=condition=ready pod -l app=postgres -n rust-api --timeout=120s
```

### 5. Desplegar Redis
```bash
kubectl apply -f k8s/04-redis.yaml

# Esperar a que esté listo
kubectl wait --for=condition=ready pod -l app=redis -n rust-api --timeout=60s
```

### 6. Desplegar API (3 réplicas)
```bash
# IMPORTANTE: Editar k8s/05-api-deployment.yaml
# Cambiar: image: tu-registry/rust-api:latest

kubectl apply -f k8s/05-api-deployment.yaml

# Esperar a que estén listos
kubectl wait --for=condition=ready pod -l app=rust-api -n rust-api --timeout=180s
```

### 7. (Opcional) Configurar Ingress
```bash
# Solo si tienes Ingress Controller instalado
kubectl apply -f k8s/06-ingress.yaml
```

---

## 🔍 Verificación

### Ver todos los recursos
```bash
kubectl get all -n rust-api
```

### Ver logs de la API
```bash
# Logs de todos los pods
kubectl logs -l app=rust-api -n rust-api --tail=50 -f

# Logs de un pod específico
kubectl logs <pod-name> -n rust-api -f
```

### Probar la API
```bash
# Opción A: Port-forward (desarrollo)
kubectl port-forward svc/rust-api-service 8080:80 -n rust-api

# Probar
curl http://localhost:8080/health

# Opción B: LoadBalancer (producción)
kubectl get svc rust-api-service -n rust-api
# Usar la EXTERNAL-IP
curl http://<EXTERNAL-IP>/health
```

### Ver estado de los pods
```bash
kubectl get pods -n rust-api -w
```

### Ejecutar comandos dentro de un pod
```bash
# Entrar a un pod de la API
kubectl exec -it <pod-name> -n rust-api -- /bin/sh

# Entrar a Postgres
kubectl exec -it postgres-0 -n rust-api -- psql -U postgres -d rust_db
```

---

## 📊 Monitoreo

### Ver métricas de auto-scaling
```bash
kubectl get hpa -n rust-api -w
```

### Ver eventos
```bash
kubectl get events -n rust-api --sort-by='.lastTimestamp'
```

### Describir un recurso (debugging)
```bash
kubectl describe pod <pod-name> -n rust-api
kubectl describe deployment rust-api -n rust-api
```

---

## 🔄 Actualizaciones (Rolling Update)

### Actualizar imagen de la API
```bash
# Build nueva versión
docker build -t tu-registry/rust-api:v2 .
docker push tu-registry/rust-api:v2

# Update deployment
kubectl set image deployment/rust-api api=tu-registry/rust-api:v2 -n rust-api

# Ver progreso
kubectl rollout status deployment/rust-api -n rust-api
```

### Rollback si algo falla
```bash
kubectl rollout undo deployment/rust-api -n rust-api
```

---

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

---

## 🎯 Checklist de Producción

- [ ] Cambiar `image` en `05-api-deployment.yaml` a tu registry
- [ ] Crear secrets con valores aleatorios (no usar defaults)
- [ ] Configurar `storageClassName` en Postgres PVC
- [ ] Ajustar `resources.limits` según tu carga
- [ ] Configurar backups de Postgres (CronJob + PVC snapshots)
- [ ] Instalar Ingress Controller
- [ ] Configurar DNS apuntando a Ingress
- [ ] Instalar Cert-Manager para SSL
- [ ] Configurar monitoreo (Prometheus + Grafana)
- [ ] Configurar alertas (Alertmanager)

---

## 🚨 Troubleshooting

### Pods en CrashLoopBackOff
```bash
kubectl logs <pod-name> -n rust-api --previous
kubectl describe pod <pod-name> -n rust-api
```

### Postgres no conecta
```bash
# Verificar que el service existe
kubectl get svc postgres-service -n rust-api

# Probar conexión desde un pod de la API
kubectl exec -it <api-pod> -n rust-api -- sh
# Dentro del pod:
nc -zv postgres-service 5432
```

### Redis no conecta
```bash
kubectl exec -it <api-pod> -n rust-api -- sh
# Dentro del pod:
nc -zv redis-service 6379
```

---

## 📈 Escalado Manual

```bash
# Escalar API a 5 réplicas
kubectl scale deployment rust-api --replicas=5 -n rust-api

# Ver estado
kubectl get pods -l app=rust-api -n rust-api
```
