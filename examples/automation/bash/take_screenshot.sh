#!/bin/bash
# A simple script to take a screenshot via Brazen Automation API
# Requires: websocat, jq, base64

URL=${1:-"ws://127.0.0.1:7942/ws"}
OUTPUT=${2:-"screenshot.png"}

if ! command -v websocat &> /dev/null; then
    echo "Error: websocat is not installed."
    exit 1
fi

if ! command -v jq &> /dev/null; then
    echo "Error: jq is not installed."
    exit 1
fi

echo "Requesting screenshot from Brazen at $URL..."

# Send the screenshot request and capture the response
# We use -n to close the connection after one message
# We use a timeout to avoid hanging
RESPONSE=$(echo '{"id":"snap","type":"screenshot"}' | websocat -n1 "$URL" 2>/dev/null)

if [ -z "$RESPONSE" ]; then
    echo "Error: No response from Brazen."
    exit 1
fi

OK=$(echo "$RESPONSE" | jq -r '.ok')

if [ "$OK" != "true" ]; then
    ERROR=$(echo "$RESPONSE" | jq -r '.error')
    echo "Error from Brazen: $ERROR"
    exit 1
fi

# Extract base64 and decode
echo "$RESPONSE" | jq -r '.result' | base64 -d > "$OUTPUT"

if [ -s "$OUTPUT" ]; then
    echo "Screenshot saved to $OUTPUT"
else
    echo "Error: Failed to save screenshot (output file is empty)."
    exit 1
fi
