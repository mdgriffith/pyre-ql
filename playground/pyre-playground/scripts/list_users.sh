#!/bin/bash

# Check if the previous script exists
if [ ! -f "./scripts/request.sh" ]; then
    echo "Error: request.sh not found in current directory"
    exit 1
fi


# Array of request data combining request IDs and payloads
declare -A SQL=(
    # List all usrs
    ["ca1ef76b8454099d6af490ed91b4ecfd2704dccc9a6d225aab00ce731b3a64ab"]='{}'

    # List all users and their accounts
    ["41ee1e1c8c03ae9b22e0716342d4b23e46805ef6f1782e4144824dd75bae3386"]='{}'
    
)

# Counter for request number
count=1
total=${#SQL[@]}

# Loop through the associative array
for request in "${!SQL[@]}"; do
    
    # Call the script with arguments
    ./scripts/request.sh -r "$request" -d "${SQL[$request]}"
    
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