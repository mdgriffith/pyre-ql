#!/bin/bash

# Check if the previous script exists
if [ ! -f "./scripts/request.sh" ]; then
    echo "Error: request.sh not found in current directory"
    exit 1
fi


# Array of request data combining request IDs and payloads
declare -A SEED_DATA=(
    # Add a new user
    ["3259ec0eb42c9eb594e95c4ebee07655505cba4c37981f4aa4f6c19ab7ef7d3e"]='{"name": "Super Mario", "status": {"type_": "Active"}}'  
)

echo "Starting seeding process..."
echo "-------------------------"

# Counter for request number
count=1
total=${#SEED_DATA[@]}

# Loop through the associative array
for request in "${!SEED_DATA[@]}"; do
    
    # Call the script with arguments
    ./scripts/request.sh -r "$request" -d "${SEED_DATA[$request]}"
    
    # Check if the last command was successful
    if [ $? -eq 0 ]; then
        echo "Request ${count} completed successfully"
    else
        echo "Request ${count} failed"
        echo "Continuing with next request..."
    fi
    
    echo "-------------------------"
    
    # Add a small delay between requests
    sleep 1
    
    ((count++))
done

echo "Seeding process completed"