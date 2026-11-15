"""Private loopback TCP relay: simulated one-way delay and bandwidth, no loss.

This shapes application byte streams, not IP packets. It cannot reproduce WAN
congestion, retransmissions, jitter, packet loss, or SSH host scheduling.
"""
import asyncio
import threading
import time


class Relay:
    def __init__(self, target_port, rtt_ms, bandwidth_kib):
        if not 0 <= rtt_ms <= 2000 or not 1 <= bandwidth_kib <= 1048576:
            raise ValueError("invalid private relay profile")
        self.target_port = target_port
        self.delay = rtt_ms / 2000
        self.rate = bandwidth_kib * 1024
        self.counts = [0, 0]
        self.connections = 0
        self.ready = threading.Event()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()
        if not self.ready.wait(5):
            raise TimeoutError("relay startup")

    async def pump(self, reader, writer, direction):
        # Bound retained application bytes to 256 KiB per direction. Separate
        # receive/send clocks avoid adding the configured delay per TCP chunk.
        queue = asyncio.Queue(maxsize=16)

        async def receive():
            due = 0
            while data := await reader.read(16384):
                due = max(time.monotonic() + self.delay, due) + len(data) / self.rate
                await queue.put((due, data))
            await queue.put((0, None))

        async def send():
            while True:
                due, data = await queue.get()
                if data is None:
                    if writer.can_write_eof():
                        writer.write_eof()
                    return
                await asyncio.sleep(max(0, due - time.monotonic()))
                writer.write(data)
                await writer.drain()
                self.counts[direction] += len(data)

        tasks = [asyncio.create_task(receive()), asyncio.create_task(send())]
        try:
            await asyncio.gather(*tasks)
        finally:
            for task in tasks:
                task.cancel()

    async def connection(self, reader, writer):
        peer = None
        tasks = []
        try:
            remote, peer = await asyncio.open_connection("127.0.0.1", self.target_port)
            self.connections += 1
            tasks = [asyncio.create_task(self.pump(reader, peer, 0)),
                     asyncio.create_task(self.pump(remote, writer, 1))]
            await asyncio.gather(*tasks)
        except (ConnectionError, OSError):
            pass
        finally:
            for task in tasks:
                task.cancel()
            writer.close()
            if peer:
                peer.close()

    def run(self):
        self.loop = asyncio.new_event_loop()
        asyncio.set_event_loop(self.loop)
        self.server = self.loop.run_until_complete(asyncio.start_server(self.connection, "127.0.0.1", 0))
        self.port = self.server.sockets[0].getsockname()[1]
        self.ready.set()
        self.loop.run_forever()
        self.server.close()
        self.loop.run_until_complete(self.server.wait_closed())
        tasks = asyncio.all_tasks(self.loop)
        for task in tasks:
            task.cancel()
        self.loop.run_until_complete(asyncio.gather(*tasks, return_exceptions=True))
        self.loop.close()

    def close(self):
        self.loop.call_soon_threadsafe(self.loop.stop)
        self.thread.join(timeout=5)
        assert not self.thread.is_alive(), "relay cleanup failed"

    def metrics(self):
        return {"kind": "local-application-stream-simulation", "configured_rtt_ms": self.delay * 2000,
                "bandwidth_kib_per_direction": self.rate / 1024,
                "client_to_server_bytes": self.counts[0], "server_to_client_bytes": self.counts[1],
                "connections": self.connections, "public_network": False,
                "packet_loss_or_retransmission_simulated": False}
