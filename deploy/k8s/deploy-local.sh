#!/bin/bash
# ============================================================================
# Script de Deployment Local en Kubernetes (Docker Desktop)
# ============================================================================

set -e  # Exit on error

echo "🚀 Iniciando deployment local en Kubernetes..."

# 1. Verificar que kubectl apunta a docker-desktop
CONTEXT=$(kubectl config current-context)
if [ "$CONTEXT" != "docker-desktop" ]; then
    echo "❌ Error: kubectl no apunta a docker-desktop (actual: $CONTEXT)"
    echo "Ejecuta: kubectl config use-context docker-desktop"
    exit 1
fi

echo "✅ Cluster: $CONTEXT"

# 2. Build de la imagen
echo "🔨 Building imagen Docker..."
docker build -t rust-api:local -f deploy/Dockerfile .

echo "✅ Imagen creada: rust-api:local"

# 3. Actualizar manifiestos para uso local
echo "📝 Actualizando manifiestos para uso local..."
sed -i '' 's|image: tu-registry/rust-api:latest|image: rust-api:local|g' deploy/k8s/05-api-deployment.yaml
sed -i '' 's|imagePullPolicy: Always|imagePullPolicy: Never|g' deploy/k8s/05-api-deployment.yaml

# 4. Crear namespace
echo "📦 Creando namespace..."
kubectl apply -f deploy/k8s/00-namespace.yaml

# 5. Crear secrets
echo "🔐 Creando secrets..."
kubectl create secret generic rust-api-secrets \
  --from-literal=jwt-secret=local_dev_secret_key \
  --from-literal=postgres-password=postgres \
  --namespace=rust-api \
  --dry-run=client -o yaml | kubectl apply -f -

# 6. Aplicar todos los manifiestos
echo "🚢 Deployando recursos..."
kubectl apply -f deploy/k8s/02-configmap.yaml
kubectl apply -f deploy/k8s/03-postgres.yaml
kubectl apply -f deploy/k8s/04-redis.yaml
kubectl apply -f deploy/k8s/05-api-deployment.yaml

# 7. Esperar a que todo esté listo
echo "⏳ Esperando a que Postgres esté listo..."
kubectl wait --for=condition=ready pod -l app=postgres -n rust-api --timeout=120s

echo "⏳ Esperando a que Redis esté listo..."
kubectl wait --for=condition=ready pod -l app=redis -n rust-api --timeout=60s

echo "⏳ Esperando a que la API esté lista..."
kubectl wait --for=condition=ready pod -l app=rust-api -n rust-api --timeout=180s

# 8. Mostrar estado
echo ""
echo "✅ Deployment completado!"
echo ""
echo "📊 Estado de los recursos:"
kubectl get all -n rust-api

echo ""
echo "🌐 Para acceder a la API:"
echo "  kubectl port-forward svc/rust-api-service 8080:80 -n rust-api"
echo "  curl http://localhost:8080/health"
echo ""
echo "📝 Ver logs:"
echo "  kubectl logs -l app=rust-api -n rust-api -f"
echo ""
echo "🧹 Para limpiar:"
echo "  kubectl delete namespace rust-api"
