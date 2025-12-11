#!/bin/bash
# ============================================================================
# BENCHMARK SCRIPT - Rust API Performance Testing
# ============================================================================
#
# Este script ejecuta benchmarks completos de la API Rust y genera un reporte.
#
# Requisitos:
#   - wrk instalado (brew install wrk)
#   - API corriendo en localhost:8080
#   - jq instalado (brew install jq)
#
# Uso:
#   ./run_benchmark.sh
#
# ============================================================================

set -e

# Colores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuración
API_URL="http://localhost:8080"
RESULTS_DIR="deploy/benchmark/results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
REPORT_FILE="$RESULTS_DIR/benchmark_${TIMESTAMP}.md"

# Crear directorio de resultados
mkdir -p "$RESULTS_DIR"

echo -e "${GREEN}🚀 Iniciando Benchmark de Rust API${NC}"
echo "=================================================="
echo ""

# ============================================================================
# FUNCIÓN: Verificar que la API está corriendo
# ============================================================================
check_api() {
    echo -e "${YELLOW}📡 Verificando que la API está disponible...${NC}"
    if curl -s -f "$API_URL/health" > /dev/null; then
        echo -e "${GREEN}✅ API disponible${NC}"
    else
        echo -e "${RED}❌ API no disponible en $API_URL${NC}"
        echo "Ejecuta: kubectl port-forward svc/rust-api-service 8080:80 -n rust-api"
        exit 1
    fi
    echo ""
}

# ============================================================================
# FUNCIÓN: Warm-up
# ============================================================================
warmup() {
    echo -e "${YELLOW}🔥 Warm-up (1000 requests)...${NC}"
    for i in {1..1000}; do
        curl -s "$API_URL/health" > /dev/null
    done
    echo -e "${GREEN}✅ Warm-up completado${NC}"
    echo ""
}

# ============================================================================
# FUNCIÓN: Benchmark de un endpoint
# ============================================================================
benchmark_endpoint() {
    local name=$1
    local url=$2
    local threads=$3
    local connections=$4
    local duration=$5
    local output_file="$RESULTS_DIR/${name}_${TIMESTAMP}.txt"
    
    echo -e "${YELLOW}📊 Benchmarking: $name${NC}"
    echo "   URL: $url"
    echo "   Threads: $threads, Connections: $connections, Duration: ${duration}s"
    
    wrk -t"$threads" -c"$connections" -d"${duration}s" --latency "$url" > "$output_file"
    
    echo -e "${GREEN}✅ Completado: $output_file${NC}"
    echo ""
}

# ============================================================================
# FUNCIÓN: Obtener métricas de Prometheus
# ============================================================================
get_prometheus_metrics() {
    echo -e "${YELLOW}📈 Obteniendo métricas de Prometheus...${NC}"
    
    # Request rate
    local req_rate=$(curl -s 'http://localhost:30900/api/v1/query?query=sum(rate(http_requests_total[1m]))' | jq -r '.data.result[0].value[1]' 2>/dev/null || echo "N/A")
    
    # Latencia p95
    local p95_latency=$(curl -s 'http://localhost:30900/api/v1/query?query=histogram_quantile(0.95,sum(rate(http_request_duration_seconds_bucket[5m]))by(le))' | jq -r '.data.result[0].value[1]' 2>/dev/null || echo "N/A")
    
    echo "   Request Rate: $req_rate req/s"
    echo "   P95 Latency: $p95_latency s"
    echo ""
}

# ============================================================================
# FUNCIÓN: Generar reporte
# ============================================================================
generate_report() {
    echo -e "${YELLOW}📝 Generando reporte...${NC}"
    
    cat > "$REPORT_FILE" << 'EOF'
# Benchmark Report - Rust API

**Fecha:** $(date)
**API:** Rust + Axum + Tokio
**Hardware:** Kubernetes local (Docker Desktop)
**Pods:** 3 réplicas

---

## Resumen Ejecutivo

EOF

    # Agregar resultados de cada benchmark
    for file in "$RESULTS_DIR"/*_${TIMESTAMP}.txt; do
        if [ -f "$file" ]; then
            echo "" >> "$REPORT_FILE"
            echo "### $(basename "$file" .txt)" >> "$REPORT_FILE"
            echo '```' >> "$REPORT_FILE"
            cat "$file" >> "$REPORT_FILE"
            echo '```' >> "$REPORT_FILE"
        fi
    done
    
    echo -e "${GREEN}✅ Reporte generado: $REPORT_FILE${NC}"
    echo ""
}

# ============================================================================
# MAIN
# ============================================================================

check_api
warmup

echo -e "${GREEN}🎯 Ejecutando Benchmarks${NC}"
echo "=================================================="
echo ""

# Escenario 1: Health Check (overhead mínimo)
benchmark_endpoint "health_light" "$API_URL/health" 4 100 30

# Escenario 2: Health Check (carga alta)
benchmark_endpoint "health_heavy" "$API_URL/health" 12 400 60

# Escenario 3: Database Query
benchmark_endpoint "users_query" "$API_URL/users" 8 200 60

# Métricas de Prometheus
get_prometheus_metrics

# Generar reporte
generate_report

echo -e "${GREEN}✅ Benchmark completado!${NC}"
echo ""
echo "📊 Resultados en: $RESULTS_DIR"
echo "📝 Reporte: $REPORT_FILE"
echo ""
echo "Para ver el reporte:"
echo "  cat $REPORT_FILE"
