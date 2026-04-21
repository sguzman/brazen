import asyncio
import json
import websockets
import sys

async def brazen_example(url="ws://127.0.0.1:7942/ws"):
    try:
        async with websockets.connect(url) as websocket:
            print(f"Connected to Brazen at {url}")

            # 1. Get a snapshot of the current state
            request = {
                "id": "req-1",
                "type": "snapshot"
            }
            await websocket.send(json.dumps(request))
            
            response = await websocket.recv()
            data = json.loads(response)
            if data.get("ok"):
                snapshot = data["result"]
                print(f"Active Tab: {snapshot['page_title']} ({snapshot['address_bar']})")
                print(f"Open Tabs: {len(snapshot['tabs'])}")
            else:
                print(f"Error getting snapshot: {data.get('error')}")

            # 2. Navigate to a new page
            print("Navigating to example.com...")
            request = {
                "id": "req-2",
                "type": "tab-navigate",
                "url": "https://example.com"
            }
            await websocket.send(json.dumps(request))
            await websocket.recv() # Wait for ack

            # 3. Subscribe to navigation events
            print("Subscribing to navigation events...")
            await websocket.send(json.dumps({
                "type": "subscribe",
                "topics": ["navigation"]
            }))

            # 4. Wait for a few events or results
            print("Listening for events (Ctrl+C to stop)...")
            while True:
                msg = await websocket.recv()
                event = json.loads(msg)
                if "topic" in event:
                    print(f"Event received: {event['topic']} -> {event.get('url', '')}")
                else:
                    print(f"Response received: {event.get('id')} ok={event.get('ok')}")

    except Exception as e:
        print(f"Connection failed: {e}")

if __name__ == "__main__":
    url = sys.argv[1] if len(sys.argv) > 1 else "ws://127.0.0.1:7942/ws"
    try:
        asyncio.run(brazen_example(url))
    except KeyboardInterrupt:
        pass
