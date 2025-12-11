#!/bin/bash
# ============================================================================
# Script de Re-deploy Rápido
# ============================================================================
# Uso: ./redeploy.sh

set -e

echo "🔨 Rebuilding imagen Docker..."
VERSION=$(date +%s)
docker build -t rust-api:$VERSION -f deploy/Dockerfile .

echo "🚀 Updating deployment en Kubernetes..."
kubectl set image deployment/rust-api api=rust-api:$VERSION -n rust-api

echo "⏳ Esperando rollout..."
kubectl rollout status deployment/rust-api -n rust-api

echo ""
echo "✅ Deploy completado!"
echo ""
echo "📊 Estado de los pods:"
kubectl get pods -l app=rust-api -n rust-api

echo ""
echo "📝 Ver logs:"
echo "  kubectl logs -l app=rust-api -n rust-api -f"
echo ""
echo "⏪ Rollback si algo falla:"
echo "  kubectl rollout undo deployment/rust-api -n rust-api"
