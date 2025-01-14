#!/bin/bash

# Default values
DEFAULT_HOST="localhost"
DEFAULT_PORT="3000"
DEFAULT_PATH="db"

# Function to display usage
usage() {
    echo "Usage: $0 -r REQUEST_STRING [-d JSON_DATA] [-h HOST] [-p PORT]"
    echo "  -r: Request string (required)"
    echo "  -d: JSON data (default: '{\"value\": \"default\"}')"
    echo "  -h: Host (default: localhost)"
    echo "  -p: Port (default: 3000)"
    exit 1
}

# Function to validate JSON
validate_json() {
    if ! jq -e . >/dev/null 2>&1 <<<"$1"; then
        echo "Error: Invalid JSON data provided"
        exit 1
    fi
}

# Parse command line arguments
while getopts "r:d:h:p:" opt; do
    case $opt in
        r) REQUEST="$OPTARG";;
        d) JSON_DATA="$OPTARG";;
        h) HOST="$OPTARG";;
        p) PORT="$OPTARG";;
        *) usage;;
    esac
done

# Check if request string is provided
if [ -z "$REQUEST" ]; then
    echo "Error: Request string (-r) is required"
    usage
fi

# Set defaults if not provided
HOST=${HOST:-$DEFAULT_HOST}
PORT=${PORT:-$DEFAULT_PORT}
JSON_DATA=${JSON_DATA:-'{"value": "default"}'}

# Validate JSON data
validate_json "$JSON_DATA"

# Construct URL
URL="http://${HOST}:${PORT}/${DEFAULT_PATH}/${REQUEST}"

# Make the curl request
echo "Sending POST request to: $URL"
echo "With data: $JSON_DATA"

curl -X POST "$URL" \
    -H "Content-Type: application/json" \
    -d "$JSON_DATA" \
    -w "\nHTTP Status Code: %{http_code}\n" \
    -s

# Check curl exit status
if [ $? -ne 0 ]; then
    echo "Error: Failed to make POST request"
    exit 1
fi