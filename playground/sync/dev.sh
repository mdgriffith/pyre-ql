#!/bin/bash

set -e

# Use the full path to pyre (aliases aren't available in non-interactive scripts)
PYRE_BIN="/Users/griff/projects/pyre/target/debug/pyre"

echo "🚀 Starting Sync Playground setup..."

# Step 1: Generate TypeScript code
echo ""
echo "📦 Step 1: Generating TypeScript code..."
"$PYRE_BIN" generate
if [ $? -ne 0 ]; then
    echo "❌ Failed to generate TypeScript code"
    exit 1
fi
echo "✅ Code generation completed"

# Step 2: Delete existing database if it exists and create fresh one
echo ""
echo "🗄️  Step 2: Setting up database..."
DB_PATH="test.db"
if [ -f "$DB_PATH" ]; then
    echo "  Deleting existing database..."
    rm "$DB_PATH"
fi

# Step 3: Run migrations and seed database using WASM
echo ""
echo "🔄 Step 3: Running migrations and seeding database..."
bun run src/init.ts
if [ $? -ne 0 ]; then
    echo "❌ Failed to run migrations or seed database"
    exit 1
fi
echo "✅ Migrations and seeding completed"

# Step 4: Start the server and Vite dev server
echo ""
echo "🌐 Step 4: Starting server and frontend..."
echo ""
echo "Server: http://localhost:3000"
echo "Frontend: http://localhost:5173"
echo ""

# Kill any existing process on port 3000
echo "Checking for existing server on port 3000..."
if lsof -ti:3000 > /dev/null 2>&1; then
    echo "  Killing existing process on port 3000..."
    lsof -ti:3000 | xargs kill -9 2>/dev/null || true
    sleep 1
fi

# Start server in background
bun run --hot src/server.ts &
SERVER_PID=$!

# Wait a moment for server to start
sleep 2

# Check if server is still running
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "❌ Server failed to start"
    exit 1
fi

# Start Vite dev server (this will block)
bunx vite

# Cleanup on exit
trap "kill $SERVER_PID 2>/dev/null" EXIT INT TERM
