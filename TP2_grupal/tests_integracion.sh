#!/bin/bash

# ==============================================================================
# YPF RUTA - SCRIPT DE PERFORMANCE Y CONCURRENCIA
# ==============================================================================

# Configuración del Test
CONCURRENT_PUMPS=50      # Cantidad de surtidores simultáneos
TX_PER_PUMP=10           # Transacciones que envía cada surtidor
CENTRAL_PORT=9000
REGIONAL_PORT=8000
STATION_PORT=7000
PUMP_BASE_PORT=6000

# Archivos de log
LOG_DIR="bench_logs"
mkdir -p $LOG_DIR

echo "🚀 [1/6] Compilando proyecto en modo RELEASE..."
cargo build --release --quiet
if [ $? -ne 0 ]; then
    echo "❌ Error de compilación."
    exit 1
fi

# Función para limpiar procesos al salir
cleanup() {
    echo ""
    echo "🛑 [6/6] Deteniendo infraestructura..."
    pkill -f "central_server"
    pkill -f "regional_server"
    pkill -f "gas_station"
    pkill -f "pump"
    echo "✅ Limpieza completada."
}
trap cleanup EXIT

echo "🏗️  [2/6] Levantando Infraestructura..."

# 1. Central (Líder ID 1)
./target/release/central_server 1 $CENTRAL_PORT > $LOG_DIR/central.log 2>&1 &
CENTRAL_PID=$!
echo "   -> Central Server (PID $CENTRAL_PID)"
sleep 2 # Esperar a que levante

# 2. Regional (Conecta al Central)
# Asumo argumentos: id port central_addr
./target/release/regional_server 1 $REGIONAL_PORT "127.0.0.1:$CENTRAL_PORT" > $LOG_DIR/regional.log 2>&1 &
REGIONAL_PID=$!
echo "   -> Regional Server (PID $REGIONAL_PID)"
sleep 2

# 3. Estación (Conecta al Regional)
# Asumo argumentos según tu gas_station.rs config
./target/release/gas_station \
    --pump-port $STATION_PORT \
    --regional-addr "127.0.0.1" \
    --regional-port $REGIONAL_PORT \
    --bulk-limit 10 \
    --bulk-timeout 2 > $LOG_DIR/station.log 2>&1 &
STATION_PID=$!
echo "   -> Gas Station (PID $STATION_PID)"
sleep 2

echo "📝 [3/6] Registrando Datos de Prueba (Admin)..."

# Preparamos un archivo de comandos para el admin
# Flujo: Login (falso para iniciar) -> Registrar Cuenta -> Obtener ID -> Registrar Tarjeta -> Obtener ID
# NOTA: Como el admin es interactivo, capturamos la salida para grepear los IDs.

# Paso A: Registrar Cuenta
INPUT_REG="REG empresa_bench \"Empresa de Benchmark\"\nEXIT\n"
echo -e "$INPUT_REG" | ./target/release/company_admin "127.0.0.1:$CENTRAL_PORT" > $LOG_DIR/admin_reg.out 2>&1

# Extraer Account ID (Buscamos el patrón "id: "X"")
ACCOUNT_ID=$(grep -oP 'id: "\K\d+' $LOG_DIR/admin_reg.out | head -1)

if [ -z "$ACCOUNT_ID" ]; then
    echo "❌ Falló el registro de cuenta. Revisa $LOG_DIR/admin_reg.out"
    exit 1
fi
echo "   -> Cuenta creada: $ACCOUNT_ID"

# Paso B: Habilitar Región (Importante para que funcione la tarjeta)
# Asumimos que el regional se registró con nombre "Region-1" o similar.
# Vamos a intentar adivinar el nombre o usar una default si tu código lo permite.
# Para este test, asumimos que el register_regional del server funcionó.
# Agregamos la región "Buenos_Aires" (ejemplo) con limite alto
INPUT_UPD="LOGIN $ACCOUNT_ID\nUPD -r Buenos_Aires:9999999\nEXIT\n"
echo -e "$INPUT_UPD" | ./target/release/company_admin "127.0.0.1:$CENTRAL_PORT" > /dev/null 2>&1

# Paso C: Registrar Tarjeta
INPUT_CARD="LOGIN $ACCOUNT_ID\nCARD 50000\nEXIT\n"
echo -e "$INPUT_CARD" | ./target/release/company_admin "127.0.0.1:$CENTRAL_PORT" > $LOG_DIR/admin_card.out 2>&1

# Extraer Card ID
CARD_ID=$(grep -oP 'id: "\K\d+' $LOG_DIR/admin_card.out | head -1)

if [ -z "$CARD_ID" ]; then
    echo "❌ Falló el registro de tarjeta. Revisa $LOG_DIR/admin_card.out"
    exit 1
fi
echo "   -> Tarjeta creada: $CARD_ID"

echo "🔥 [4/6] INICIANDO ATAQUE DE CONCURRENCIA..."
echo "   -> Surtidores: $CONCURRENT_PUMPS"
echo "   -> Transacciones por surtidor: $TX_PER_PUMP"
echo "   -> Total Transacciones: $((CONCURRENT_PUMPS * TX_PER_PUMP))"

START_TIME=$(date +%s.%N)

# Lanzar N surtidores en paralelo
for i in $(seq 1 $CONCURRENT_PUMPS); do
    MY_PORT=$((PUMP_BASE_PORT + i))
    # Ejecutar pump en background. 
    # Asumo que tu pump tiene un modo o loop. Si es interactivo, habría que adaptarlo.
    # Aquí simulamos el envío usando el API TCP que implementaste en pump.rs o lanzando el binario.
    # Si el binario pump envía solo en loop, lo lanzamos:
    
    ./target/release/pump \
        --station-addr "127.0.0.1:$STATION_PORT" \
        --response-port $MY_PORT \
        --local-addr "127.0.0.1" > $LOG_DIR/pump_$i.log 2>&1 &
done

# Esperar a que terminen (o matar después de un tiempo si son loops infinitos)
# Como tu Pump parece tener un loop infinito con sleep, vamos a dejar correr el test X segundos
TEST_DURATION=10
echo "   -> Corriendo carga por $TEST_DURATION segundos..."
sleep $TEST_DURATION

END_TIME=$(date +%s.%N)
DURATION=$(echo "$END_TIME - $START_TIME" | bc)

echo "📊 [5/6] Resultados de Performance"

# Contar éxitos en los logs de los pumps
ACCEPTED_COUNT=$(grep -r "TX.*Accepted" $LOG_DIR/pump_*.log | wc -l)
REJECTED_COUNT=$(grep -r "TX.*Rechazada" $LOG_DIR/pump_*.log | wc -l)
TOTAL_PROCESSED=$((ACCEPTED_COUNT + REJECTED_COUNT))

# Calcular TPS (estimado)
TPS=$(echo "$TOTAL_PROCESSED / $DURATION" | bc -l)

echo "=================================================="
echo " RESULTADOS FINALES"
echo "=================================================="
echo " Tiempo Total:      LC_NUMERIC=C printf '%.2f' $DURATION segs"
echo " Transacciones OK:  $ACCEPTED_COUNT"
echo " Transacciones NO:  $REJECTED_COUNT"
echo " Total Procesado:   $TOTAL_PROCESSED"
echo " Throughput:        LC_NUMERIC=C printf '%.2f' $TPS TX/s"
echo "=================================================="
echo "Logs detallados disponibles en $LOG_DIR/"