#!/bin/bash

# AOR Health Monitor Script
# This script monitors the health of your AOR backend service and restarts it if unhealthy

# Configuration
COMPOSE_PROJECT_NAME="aor-backend"
COMPOSE_FILE="docker-compose.yaml"
SERVICE_NAME="server"
LOG_FILE="/var/log/aor-health-monitor.log"
MAX_RESTART_ATTEMPTS=3
RESTART_COOLDOWN=300  # 5 minutes between restart attempts

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to log with timestamp
log() {
    echo -e "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$LOG_FILE"
}

# Function to check if service is healthy
check_health() {
    local container_id=$(docker-compose -f "$COMPOSE_FILE" ps -q "$SERVICE_NAME" 2>/dev/null)
    
    if [ -z "$container_id" ]; then
        log "${RED}ERROR: Container $SERVICE_NAME not found${NC}"
        return 1
    fi
    
    # Check container status
    local container_status=$(docker inspect --format='{{.State.Status}}' "$container_id" 2>/dev/null)
    if [ "$container_status" != "running" ]; then
        log "${RED}ERROR: Container $SERVICE_NAME is not running (status: $container_status)${NC}"
        return 1
    fi
    
    # Check Docker health check
    local health_status=$(docker inspect --format='{{.State.Health.Status}}' "$container_id" 2>/dev/null)
    if [ "$health_status" = "unhealthy" ]; then
        log "${RED}ERROR: Container $SERVICE_NAME is unhealthy${NC}"
        return 1
    elif [ "$health_status" = "healthy" ]; then
        log "${GREEN}SUCCESS: Container $SERVICE_NAME is healthy${NC}"
        return 0
    elif [ "$health_status" = "starting" ]; then
        log "${YELLOW}INFO: Container $SERVICE_NAME is still starting up${NC}"
        return 2  # Still starting, don't restart yet
    else
        log "${YELLOW}WARNING: Container $SERVICE_NAME health status unknown: $health_status${NC}"
        return 1
    fi
}

# Function to check API endpoint health
check_api_health() {
    local health_url="http://localhost:8000/game/leaderboard"
    local response_code=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 10 --max-time 30 "$health_url" 2>/dev/null)
    
    if [ "$response_code" = "200" ]; then
        log "${GREEN}SUCCESS: API endpoint responding correctly (HTTP $response_code)${NC}"
        return 0
    else
        log "${RED}ERROR: API endpoint unhealthy (HTTP $response_code)${NC}"
        return 1
    fi
}

# Function to check database connectivity
check_db_health() {
    local db_container=$(docker-compose -f "$COMPOSE_FILE" ps -q db 2>/dev/null)
    if [ -z "$db_container" ]; then
        log "${RED}ERROR: Database container not found${NC}"
        return 1
    fi
    
    local db_status=$(docker inspect --format='{{.State.Status}}' "$db_container" 2>/dev/null)
    if [ "$db_status" != "running" ]; then
        log "${RED}ERROR: Database container not running (status: $db_status)${NC}"
        return 1
    fi
    
    # Test database connection
    if docker exec "$db_container" pg_isready -U aot >/dev/null 2>&1; then
        log "${GREEN}SUCCESS: Database is responding${NC}"
        return 0
    else
        log "${RED}ERROR: Database not responding${NC}"
        return 1
    fi
}

# Function to restart the service
restart_service() {
    local restart_file="/tmp/aor_restart_count"
    local current_time=$(date +%s)
    
    # Check restart attempts in the last hour
    if [ -f "$restart_file" ]; then
        local last_restart_time=$(cat "$restart_file" | tail -1 | cut -d: -f1)
        local restart_count=$(cat "$restart_file" | wc -l)
        
        if [ $((current_time - last_restart_time)) -lt "$RESTART_COOLDOWN" ]; then
            log "${YELLOW}WARNING: Recently restarted, waiting for cooldown period${NC}"
            return 1
        fi
        
        # Clean old entries (older than 1 hour)
        local temp_file=$(mktemp)
        while IFS=: read -r timestamp; do
            if [ $((current_time - timestamp)) -lt 3600 ]; then
                echo "$timestamp" >> "$temp_file"
            fi
        done < "$restart_file"
        mv "$temp_file" "$restart_file"
        
        restart_count=$(cat "$restart_file" | wc -l)
        
        if [ "$restart_count" -ge "$MAX_RESTART_ATTEMPTS" ]; then
            log "${RED}ERROR: Maximum restart attempts ($MAX_RESTART_ATTEMPTS) reached in the last hour. Manual intervention required.${NC}"
            return 1
        fi
    fi
    
    log "${YELLOW}INFO: Attempting to restart $SERVICE_NAME service${NC}"
    
    # Record restart attempt
    echo "$current_time" >> "$restart_file"
    
    # Stop and start the service
    if docker-compose -f "$COMPOSE_FILE" stop "$SERVICE_NAME" >/dev/null 2>&1; then
        log "${GREEN}SUCCESS: Stopped $SERVICE_NAME${NC}"
        sleep 5
        
        if docker-compose -f "$COMPOSE_FILE" up -d "$SERVICE_NAME" >/dev/null 2>&1; then
            log "${GREEN}SUCCESS: Started $SERVICE_NAME${NC}"
            sleep 30  # Wait for service to initialize
            return 0
        else
            log "${RED}ERROR: Failed to start $SERVICE_NAME${NC}"
            return 1
        fi
    else
        log "${RED}ERROR: Failed to stop $SERVICE_NAME${NC}"
        return 1
    fi
}

# Function to send notification (optional - configure webhook/email)
send_notification() {
    local message="$1"
    local severity="$2"
    log "${YELLOW}NOTIFICATION: [$severity] $message${NC}"
}

# Main health check function
main() {
    log "${YELLOW}INFO: Starting health check${NC}"
    
    local health_issues=0
    
    # Check container health
    check_health
    local container_health=$?
    
    if [ $container_health -eq 2 ]; then
        log "${YELLOW}INFO: Container still starting, skipping other checks${NC}"
        return 0
    elif [ $container_health -ne 0 ]; then
        health_issues=$((health_issues + 1))
    fi
    
    # Check API health
    if [ $container_health -eq 0 ]; then
        check_api_health
        if [ $? -ne 0 ]; then
            health_issues=$((health_issues + 1))
        fi
    fi
    
    # Check database health
    check_db_health
    if [ $? -ne 0 ]; then
        health_issues=$((health_issues + 1))
    fi
    
    # Take action based on health status
    if [ $health_issues -gt 0 ]; then
        log "${RED}ERROR: Found $health_issues health issues${NC}"
        send_notification "Service unhealthy, attempting restart" "ERROR"
        
        if restart_service; then
            log "${GREEN}SUCCESS: Service restarted successfully${NC}"
            send_notification "Service restarted successfully" "INFO"
        else
            log "${RED}ERROR: Failed to restart service${NC}"
            send_notification "Failed to restart service - manual intervention required" "CRITICAL"
        fi
    else
        log "${GREEN}SUCCESS: All health checks passed${NC}"
    fi
}

# Ensure log directory exists
mkdir -p "$(dirname "$LOG_FILE")"

# Change to the directory containing docker-compose.yaml
cd "$(dirname "$0")" || {
    log "${RED}ERROR: Cannot change to script directory${NC}"
    exit 1
}

# Run main function
main

log "${YELLOW}INFO: Health check completed${NC}"