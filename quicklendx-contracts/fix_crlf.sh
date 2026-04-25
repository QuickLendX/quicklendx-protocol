#!/bin/bash
# Find all .rs files with CRLF line endings and convert them to LF
cd /mnt/c/Users/fuhad/quicklendx-protocol/quicklendx-contracts/src
echo "Files with CRLF:"
grep -rl $'\r' . --include="*.rs" | tee /tmp/crlf_files.txt
echo ""
echo "Converting..."
while IFS= read -r f; do
    sed -i 's/\r//' "$f"
    echo "Fixed: $f"
done < /tmp/crlf_files.txt
echo "Done."
