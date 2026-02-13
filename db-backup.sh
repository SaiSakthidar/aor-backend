#!/bin/bash

# AOR Database Backup Script
# This script creates automated backups of the PostgreSQL database every 6 hours

# Configuration
COMPOSE_FILE="docker-compose.yaml"
DB_SERVICE_NAME="db"
DB_USER="aot"
BACKUP_DIR="/var/backups/aor-database"
LOG_FILE="/var/log/aor-backup.log"
RETENTION_DAYS=30  # Keep backups for 30 days
COMPRESSION_LEVEL=6  # gzip compression level (1-9, 9 is highest)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to log with timestamp
log() {
    echo -e "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$LOG_FILE"
}

# Function to create backup directory
setup_backup_dir() {
    if [ ! -d "$BACKUP_DIR" ]; then
        mkdir -p "$BACKUP_DIR"
        if [ $? -eq 0 ]; then
            log "${GREEN}SUCCESS: Created backup directory: $BACKUP_DIR${NC}"
        else
            log "${RED}ERROR: Failed to create backup directory: $BACKUP_DIR${NC}"
            exit 1
        fi
    fi
}

# Function to check if database container is running
check_db_container() {
    local db_container=$(docker-compose -f "$COMPOSE_FILE" ps -q "$DB_SERVICE_NAME" 2>/dev/null)
    
    if [ -z "$db_container" ]; then
        log "${RED}ERROR: Database container '$DB_SERVICE_NAME' not found${NC}"
        return 1
    fi
    
    local db_status=$(docker inspect --format='{{.State.Status}}' "$db_container" 2>/dev/null)
    if [ "$db_status" != "running" ]; then
        log "${RED}ERROR: Database container not running (status: $db_status)${NC}"
        return 1
    fi
    
    # Test database connectivity
    if ! docker exec "$db_container" pg_isready -U "$DB_USER" >/dev/null 2>&1; then
        log "${RED}ERROR: Database not responding to connection attempts${NC}"
        return 1
    fi
    
    log "${GREEN}SUCCESS: Database container is healthy and responding${NC}"
    return 0
}

# Function to get database size
get_db_size() {
    local db_container=$(docker-compose -f "$COMPOSE_FILE" ps -q "$DB_SERVICE_NAME" 2>/dev/null)
    local db_size=$(docker exec "$db_container" psql -U "$DB_USER" -t -c "SELECT pg_size_pretty(pg_database_size('$DB_USER'));" 2>/dev/null | xargs)
    
    if [ -n "$db_size" ]; then
        log "${GREEN}INFO: Current database size: $db_size${NC}"
    else
        log "${YELLOW}WARNING: Could not determine database size${NC}"
    fi
}

# Function to create database backup
create_backup() {
    local timestamp=$(date '+%Y%m%d_%H%M%S')
    local backup_filename="aor_backup_${timestamp}.sql"
    local backup_path="$BACKUP_DIR/$backup_filename"
    local compressed_backup_path="${backup_path}.gz"
    
    log "${YELLOW}INFO: Starting database backup...${NC}"
    
    # Get database container ID
    local db_container=$(docker-compose -f "$COMPOSE_FILE" ps -q "$DB_SERVICE_NAME")
    
    # Create the backup using pg_dump
    log "${YELLOW}INFO: Creating SQL dump...${NC}"
    if docker exec "$db_container" pg_dump -U "$DB_USER" -h localhost -p 5432 --verbose --clean --no-owner --no-acl --format=plain "$DB_USER" > "$backup_path" 2>/dev/null; then
        log "${GREEN}SUCCESS: SQL dump created: $backup_filename${NC}"
    else
        log "${RED}ERROR: Failed to create SQL dump${NC}"
        return 1
    fi
    
    # Verify backup file was created and has content
    if [ ! -s "$backup_path" ]; then
        log "${RED}ERROR: Backup file is empty or was not created${NC}"
        rm -f "$backup_path"
        return 1
    fi
    
    local backup_size=$(du -h "$backup_path" | cut -f1)
    log "${GREEN}INFO: Backup file size: $backup_size${NC}"
    
    # Compress the backup
    log "${YELLOW}INFO: Compressing backup...${NC}"
    if gzip -"$COMPRESSION_LEVEL" "$backup_path"; then
        local compressed_size=$(du -h "$compressed_backup_path" | cut -f1)
        log "${GREEN}SUCCESS: Backup compressed: ${backup_filename}.gz (size: $compressed_size)${NC}"
    else
        log "${RED}ERROR: Failed to compress backup${NC}"
        return 1
    fi
    
    # Verify compressed backup
    if ! gzip -t "$compressed_backup_path" 2>/dev/null; then
        log "${RED}ERROR: Compressed backup file is corrupted${NC}"
        return 1
    fi
    
    log "${GREEN}SUCCESS: Backup completed successfully: ${backup_filename}.gz${NC}"
    return 0
}

# Function to clean up old backups
cleanup_old_backups() {
    log "${YELLOW}INFO: Cleaning up backups older than $RETENTION_DAYS days...${NC}"
    
    local deleted_count=0
    local total_space_freed=0
    
    # Find and delete old backup files
    while IFS= read -r -d '' backup_file; do
        if [ -f "$backup_file" ]; then
            local file_size=$(stat -c%s "$backup_file" 2>/dev/null || echo 0)
            rm -f "$backup_file"
            deleted_count=$((deleted_count + 1))
            total_space_freed=$((total_space_freed + file_size))
            log "${YELLOW}INFO: Deleted old backup: $(basename "$backup_file")${NC}"
        fi
    done < <(find "$BACKUP_DIR" -name "aor_backup_*.sql.gz" -type f -mtime +$RETENTION_DAYS -print0)
    
    if [ $deleted_count -gt 0 ]; then
        local space_freed_mb=$((total_space_freed / 1024 / 1024))
        log "${GREEN}SUCCESS: Deleted $deleted_count old backup(s), freed ${space_freed_mb}MB${NC}"
    else
        log "${GREEN}INFO: No old backups to clean up${NC}"
    fi
}

# Function to list current backups
list_backups() {
    local backup_count=$(find "$BACKUP_DIR" -name "aor_backup_*.sql.gz" -type f | wc -l)
    local total_size=$(du -sh "$BACKUP_DIR" 2>/dev/null | cut -f1 || echo "0")
    
    log "${GREEN}INFO: Current backup status: $backup_count backup(s), total size: $total_size${NC}"
    
    if [ $backup_count -gt 0 ]; then
        log "${GREEN}INFO: Recent backups:${NC}"
        find "$BACKUP_DIR" -name "aor_backup_*.sql.gz" -type f -printf "%T@ %Tc %p\n" | sort -n | tail -5 | while read timestamp date_str filepath; do
            local file_size=$(du -h "$filepath" | cut -f1)
            local filename=$(basename "$filepath")
            log "${GREEN}  - $filename ($file_size) - $date_str${NC}"
        done
    fi
}

# Function to test backup integrity
test_backup_integrity() {
    local latest_backup=$(find "$BACKUP_DIR" -name "aor_backup_*.sql.gz" -type f -printf "%T@ %p\n" | sort -n | tail -1 | cut -d' ' -f2-)
    
    if [ -n "$latest_backup" ] && [ -f "$latest_backup" ]; then
        log "${YELLOW}INFO: Testing integrity of latest backup...${NC}"
        
        # Test gzip integrity
        if gzip -t "$latest_backup" 2>/dev/null; then
            log "${GREEN}SUCCESS: Latest backup file integrity verified${NC}"
            return 0
        else
            log "${RED}ERROR: Latest backup file is corrupted${NC}"
            return 1
        fi
    else
        log "${YELLOW}WARNING: No backup files found to test${NC}"
        return 1
    fi
}

# Function to send notification (optional)
send_notification() {
    local message="$1"
    local severity="$2"
    log "${YELLOW}NOTIFICATION: [$severity] $message${NC}"
}

# Main backup function
main() {
    log "${YELLOW}INFO: Starting database backup process${NC}"
    
    # Setup backup directory
    setup_backup_dir
    
    # Check database health
    if ! check_db_container; then
        send_notification "Database backup failed - database container not healthy" "ERROR"
        exit 1
    fi
    
    # Get current database size
    get_db_size
    
    # Create backup
    if create_backup; then
        log "${GREEN}SUCCESS: Database backup completed successfully${NC}"
        send_notification "Database backup completed successfully" "INFO"
        
        # Test backup integrity
        test_backup_integrity
        
        # Clean up old backups
        cleanup_old_backups
        
        # List current backups
        list_backups
        
    else
        log "${RED}ERROR: Database backup failed${NC}"
        send_notification "Database backup failed - check logs for details" "ERROR"
        exit 1
    fi
    
    log "${GREEN}SUCCESS: Backup process completed${NC}"
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