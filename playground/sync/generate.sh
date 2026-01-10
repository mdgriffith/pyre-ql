#!/bin/bash

set -e

# Use the full path to pyre (aliases aren't available in non-interactive scripts)
PYRE_BIN="/Users/griff/projects/pyre/target/debug/pyre"

echo "🚀 Starting Terminal Sync Playground setup..."

# Step 1: Generate TypeScript code
echo ""
echo "📦 Step 1: Generating TypeScript code..."
"$PYRE_BIN" generate
if [ $? -ne 0 ]; then
    echo "❌ Failed to generate TypeScript code"
    exit 1
fi
echo "✅ Code generation completed"
