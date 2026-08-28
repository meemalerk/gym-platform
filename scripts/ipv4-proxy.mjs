/**
 * Minimal TCP proxy: binds IPv4 0.0.0.0:<listen> and forwards to 127.0.0.1:<target>.
 *
 * Needed because Metro binds to IPv6 (::) and WSL2's localhostForwarding only
 * created a relay for the IPv4-bound backend, leaving the web server unreachable
 * from Windows.
 */
import net from 'node:net';

const LISTEN = Number(process.argv[2] ?? 8212);
const TARGET = Number(process.argv[3] ?? 8210);

const server = net.createServer((client) => {
  const upstream = net.connect(TARGET, '127.0.0.1');
  client.pipe(upstream);
  upstream.pipe(client);
  const drop = () => {
    client.destroy();
    upstream.destroy();
  };
  client.on('error', drop);
  upstream.on('error', drop);
});

// '0.0.0.0' forces IPv4 so WSL creates a localhost relay for it.
server.listen(LISTEN, '0.0.0.0', () => {
  console.log(`ipv4 proxy listening 0.0.0.0:${LISTEN} -> 127.0.0.1:${TARGET}`);
});
