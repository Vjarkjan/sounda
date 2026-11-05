// Interactive, reproducible REA MCP driver. Target bytes are data, never commands.
// Run with GHIDRA_INSTALL_DIR and JAVA_HOME set, then provide one JSON request
// per stdin line: {"tool":"open_binary","arguments":{...},"save":"name"}.
// Full inline Evidence is retained in output/research/rea; stdout is compact.
import { createInterface } from 'node:readline';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';

const packageRoot = process.env.REA_PACKAGE_ROOT;
if (!packageRoot) throw new Error('Set REA_PACKAGE_ROOT to the installed rea-agents package directory');
const { Client } = await import(resolve(packageRoot, '../@modelcontextprotocol/client/dist/index.mjs'));
const { StdioClientTransport } = await import(resolve(packageRoot, '../@modelcontextprotocol/client/dist/stdio.mjs'));
const output = resolve('output/research/rea');
await mkdir(output, { recursive: true });
const client = new Client({ name: 'sounda-dsp-investigation', version: '1.0.0' }, { capabilities: {} });
const transport = new StdioClientTransport({ command: process.execPath, args: [join(packageRoot, 'scripts/rea.mjs'), 'mcp'], env: process.env, stderr: 'inherit' });
await client.connect(transport);
const tools = await client.listTools();
await writeFile(join(output, 'tool-schemas.json'), JSON.stringify(tools, null, 2));
console.log(JSON.stringify({ ready: true, tools: tools.tools.length }));
const lines = createInterface({ input: process.stdin, terminal: false });
try {
  for await (const line of lines) {
    if (!line.trim()) continue;
    if (line === 'exit') break;
    try {
      const request = JSON.parse(line);
      const result = await client.callTool({ name: request.tool, arguments: request.arguments || {} }, { timeout: 900000 });
      const name = request.save || `${Date.now()}-${request.tool}`;
      const path = join(output, name.replace(/[^a-zA-Z0-9_.-]/g, '_') + '.json');
      await writeFile(path, JSON.stringify(result, null, 2));
      let parsed = result.structuredContent;
      if (!parsed) {
        const text = result.content?.find(c => c.type === 'text')?.text;
        try { parsed = JSON.parse(text); } catch { parsed = text; }
      }
      const evidence = parsed?.evidence_id || parsed?.result?.evidence_id;
      const preview = JSON.stringify(parsed);
      console.log(JSON.stringify({ tool: request.tool, path, isError: result.isError || false, evidence_id: evidence, preview: preview?.slice(0, request.preview || 800) }));
    } catch (error) { console.log(JSON.stringify({ error: error.message })); }
  }
} finally {
  lines.close();
  process.stdin.pause();
  await client.close();
}
