#!/bin/bash

echo "=== ALGOL26 Project Check ==="
echo ""

# Check directory structure
echo "📁 Project Structure:"
tree -L 2 -I "target|.git" 2>/dev/null || find . -maxdepth 2 -not -path "./target*" -not -path "./.git*" | sort
echo ""

# Check source files
echo "📄 Source Files:"
for file in src/*.rs; do
    if [ -f "$file" ]; then
        lines=$(wc -l < "$file")
        echo "  $file: $lines lines"
    fi
done
echo ""

# Check documentation
echo "📚 Documentation:"
for file in docs/*.md README.md; do
    if [ -f "$file" ]; then
        lines=$(wc -l < "$file")
        echo "  $file: $lines lines"
    fi
done
echo ""

# Check examples
echo "💻 Examples:"
for file in examples/*.gol; do
    if [ -f "$file" ]; then
        lines=$(wc -l < "$file")
        echo "  $file: $lines lines"
    fi
done
echo ""

# Check tests
echo "🧪 Tests:"
for file in tests/*.rs; do
    if [ -f "$file" ]; then
        lines=$(wc -l < "$file")
        echo "  $file: $lines lines"
    fi
done
echo ""

# Try to build
echo "🔨 Building..."
cargo build 2>&1 | tail -5
echo ""

# Show git status
echo "📝 Git Status:"
git status --short 2>/dev/null || echo "Not a git repository"
echo ""

echo "=== Check Complete ==="
