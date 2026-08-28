/**
 * One LAN door for both Metro and the API.
 *
 * WSL2 exposes ports to Windows *localhost*, but reaching them from a phone on
 * the LAN needs a `netsh portproxy` rule, which needs admin. This machine
 * already has exactly one such rule we can use — port 8080 — so rather than ask
 * for elevation we put a reverse proxy on it and route by path:
 *
 *     /api/*, /health, /ready, /swagger-ui, /api-docs  ->  gym API
 *     everything else                                  ->  Metro
 *
 * The one subtlety: Expo's manifest tells the phone where to fetch the bundle,
 * and Metro advertises its own port (8213). The phone can only reach 8080, so we
 * rewrite that port in the manifest as it passes through.
 *
 * Usage: node scripts/lan-proxy.mjs [listen] [metroPort] [apiPort]
 */
import http from 'node:http';
import net from 'node:net';

const LISTEN = Number(process.argv[2] ?? 8080);
const METRO = Number(process.argv[3] ?? 8213);
const API = Number(process.argv[4] ?? 8090);

const API_PREFIXES = ['/api/', '/api-docs', '/health', '/ready', '/swagger-ui'];
const isApi = (url) => API_PREFIXES.some((p) => url === p || url.startsWith(p));

/** Metro's manifest embeds `host:8213`; the phone can only reach `host:8080`. */
const rewritePort = (text) =>
  text.split(`:${METRO}`).join(`:${LISTEN}`);

const server = http.createServer((req, res) => {
  const target = isApi(req.url) ? API : METRO;

  const upstream = http.request(
    { host: '127.0.0.1', port: target, path: req.url, method: req.method, headers: req.headers },
    (up) => {
      const type = up.headers['content-type'] ?? '';
      // Only the manifest needs rewriting, and Expo serves it as `text/plain`
      // (not JSON — checking for JSON silently does nothing). Explicitly exclude
      // javascript so the 8 MB bundle is streamed, never buffered.
      const rewrite =
        target === METRO &&
        !type.includes('javascript') &&
        (type.includes('json') || type.includes('text/plain'));

      if (!rewrite) {
        res.writeHead(up.statusCode ?? 502, up.headers);
        up.pipe(res);
        return;
      }

      const chunks = [];
      up.on('data', (c) => chunks.push(c));
      up.on('end', () => {
        const patched = rewritePort(Buffer.concat(chunks).toString('utf8'));
        const headers = { ...up.headers };
        delete headers['content-length']; // length changed
        res.writeHead(up.statusCode ?? 502, headers);
        res.end(patched);
      });
    },
  );

  upstream.on('error', (err) => {
    res.writeHead(502, { 'content-type': 'text/plain' });
    res.end(`proxy: upstream ${target} unreachable (${err.code})`);
  });

  req.pipe(upstream);
});

// Metro uses WebSockets for logs and HMR; without this they fail silently.
server.on('upgrade', (req, socket, head) => {
  const upstream = net.connect(METRO, '127.0.0.1', () => {
    upstream.write(
      `${req.method} ${req.url} HTTP/1.1\r\n` +
        Object.entries(req.headers)
          .map(([k, v]) => `${k}: ${v}\r\n`)
          .join('') +
        '\r\n',
    );
    upstream.write(head);
    socket.pipe(upstream);
    upstream.pipe(socket);
  });
  const drop = () => {
    socket.destroy();
    upstream.destroy();
  };
  upstream.on('error', drop);
  socket.on('error', drop);
});

// IPv4 bind so the existing Windows portproxy can reach it.
server.listen(LISTEN, '0.0.0.0', () => {
  console.log(`lan-proxy :${LISTEN}  ->  metro :${METRO} | api :${API}`);
});
