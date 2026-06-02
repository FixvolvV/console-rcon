from fastapi import FastAPI, WebSocket

app = FastAPI()


@app.websocket("/server/rcon/connect")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    while True:
        print(await websocket.receive())
