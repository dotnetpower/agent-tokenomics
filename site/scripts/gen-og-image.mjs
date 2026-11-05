// gen-og-image.mjs - render the Open Graph / Twitter social preview cover image (1200x630).
// Run via `npm --prefix site run gen-og` to regenerate `site/public/og-cover.png`.

import { fileURLToPath } from "node:url";
import path from "node:path";
import sharp from "sharp";

const WIDTH = 1200;
const HEIGHT = 630;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.resolve(scriptDir, "..", "public", "og-cover.png");

const svg = `<svg width="${WIDTH}" height="${HEIGHT}" viewBox="0 0 ${WIDTH} ${HEIGHT}" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#0b0e18"/>
      <stop offset="55%" stop-color="#111629"/>
      <stop offset="100%" stop-color="#161d38"/>
    </linearGradient>
    <radialGradient id="glowA" cx="20%" cy="22%" r="55%">
      <stop offset="0%" stop-color="#3b5bdb" stop-opacity="0.34"/>
      <stop offset="100%" stop-color="#3b5bdb" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="glowB" cx="84%" cy="76%" r="50%">
      <stop offset="0%" stop-color="#20c997" stop-opacity="0.22"/>
      <stop offset="100%" stop-color="#20c997" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="titleGrad" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stop-color="#ffffff"/>
      <stop offset="60%" stop-color="#dbe4ff"/>
      <stop offset="100%" stop-color="#748ffc"/>
    </linearGradient>
    <linearGradient id="accentGrad" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stop-color="#748ffc"/>
      <stop offset="100%" stop-color="#63e6be"/>
    </linearGradient>
    <pattern id="grid" width="40" height="40" patternUnits="userSpaceOnUse">
      <path d="M 40 0 L 0 0 0 40" fill="none" stroke="#ffffff" stroke-opacity="0.04" stroke-width="1"/>
    </pattern>
  </defs>

  <rect width="${WIDTH}" height="${HEIGHT}" fill="url(#bg)"/>
  <rect width="${WIDTH}" height="${HEIGHT}" fill="url(#glowA)"/>
  <rect width="${WIDTH}" height="${HEIGHT}" fill="url(#glowB)"/>
  <rect width="${WIDTH}" height="${HEIGHT}" fill="url(#grid)"/>

  <!-- Top border accent -->
  <rect x="0" y="0" width="${WIDTH}" height="5" fill="url(#accentGrad)"/>

  <!-- Eyebrow pill -->
  <rect x="72" y="58" width="748" height="36" rx="18" fill="#1b2340" stroke="#364fc7" stroke-width="1.2"/>
  <circle cx="94" cy="76" r="5" fill="#63e6be"/>
  <text x="110" y="81" font-family="'DejaVu Sans Mono', 'Courier New', monospace" font-size="14" font-weight="700" letter-spacing="1.4" fill="#dbe4ff">
    TOKENOMICS &amp; TOKEN ECONOMICS · LLM &amp; AI AGENT PLAYBOOK (EN / KO)
  </text>

  <!-- Brand title & tagline -->
  <text x="72" y="176" font-family="'DejaVu Sans', 'Segoe UI', Arial, sans-serif" font-size="74" font-weight="700" letter-spacing="-1" fill="url(#titleGrad)">
    agent-tokenomics
  </text>
  <text x="72" y="232" font-family="'DejaVu Sans', 'Segoe UI', Arial, sans-serif" font-size="31" font-weight="600" fill="url(#accentGrad)">
    Spend fewer tokens, without making your AI tools worse.
  </text>
  <text x="72" y="278" font-family="'DejaVu Sans', 'Segoe UI', Arial, sans-serif" font-size="20" fill="#b2b8cc">
    Tokenomics (Token Economics) for VS Code GitHub Copilot (cctx) &amp; LLM Agents (tokprof).
  </text>

  <!-- 3 Feature Cards -->
  <g transform="translate(72, 322)">
    <!-- Card 1: Track A -->
    <rect x="0" y="0" width="336" height="196" rx="16" fill="#141a2e" fill-opacity="0.92" stroke="#2e3a5c" stroke-width="1.5"/>
    <text x="24" y="36" font-family="'DejaVu Sans Mono', monospace" font-size="13" font-weight="700" letter-spacing="1.2" fill="#748ffc">
      TRACK A · VS CODE COPILOT
    </text>
    <text x="24" y="70" font-family="'DejaVu Sans', sans-serif" font-size="21" font-weight="700" fill="#f6f8ff">
      Copilot Tokenomics
    </text>
    <text x="24" y="106" font-family="'DejaVu Sans', sans-serif" font-size="15" fill="#bdc4da">
      • AI Credits &amp; multiplier billing
    </text>
    <text x="24" y="132" font-family="'DejaVu Sans', sans-serif" font-size="15" fill="#bdc4da">
      • Context hygiene &amp; MCP audit
    </text>
    <text x="24" y="158" font-family="'DejaVu Sans', sans-serif" font-size="15" fill="#bdc4da">
      • BPE-aligned formula prompts
    </text>

    <!-- Card 2: Track B -->
    <rect x="360" y="0" width="336" height="196" rx="16" fill="#141a2e" fill-opacity="0.92" stroke="#2e3a5c" stroke-width="1.5"/>
    <text x="384" y="36" font-family="'DejaVu Sans Mono', monospace" font-size="13" font-weight="700" letter-spacing="1.2" fill="#63e6be">
      TRACK B · AGENT ENGINEERING
    </text>
    <text x="384" y="70" font-family="'DejaVu Sans', sans-serif" font-size="21" font-weight="700" fill="#f6f8ff">
      Agent Tokenomics
    </text>
    <text x="384" y="106" font-family="'DejaVu Sans', sans-serif" font-size="15" fill="#bdc4da">
      • Diagnose → Isolate → Prove
    </text>
    <text x="384" y="132" font-family="'DejaVu Sans', sans-serif" font-size="15" fill="#bdc4da">
      • Improve → Result &amp; Rubric
    </text>
    <text x="384" y="158" font-family="'DejaVu Sans', sans-serif" font-size="15" fill="#bdc4da">
      • Golden dataset quality guardrails
    </text>

    <!-- Card 3: tokprof -->
    <rect x="720" y="0" width="336" height="196" rx="16" fill="#141a2e" fill-opacity="0.92" stroke="#364fc7" stroke-width="1.5"/>
    <text x="744" y="36" font-family="'DejaVu Sans Mono', monospace" font-size="13" font-weight="700" letter-spacing="1.2" fill="#ffd43b">
      RUST + EBPF PROFILER · PYPI
    </text>
    <text x="744" y="70" font-family="'DejaVu Sans', sans-serif" font-size="21" font-weight="700" fill="#f6f8ff">
      pip install tokprof
    </text>
    <text x="744" y="106" font-family="'DejaVu Sans', sans-serif" font-size="15" fill="#bdc4da">
      • Zero-code per-call breakdown
    </text>
    <text x="744" y="132" font-family="'DejaVu Sans', sans-serif" font-size="15" fill="#bdc4da">
      • System / History / Tools / Cache
    </text>
    <text x="744" y="158" font-family="'DejaVu Sans', sans-serif" font-size="15" fill="#bdc4da">
      • Interactive HTML Edition Dossier
    </text>
  </g>

  <!-- Footer bar -->
  <text x="72" y="578" font-family="'DejaVu Sans Mono', monospace" font-size="18" fill="#8c94b0">
    https://dotnetpower.github.io/agent-tokenomics/
  </text>
  <text x="1128" y="578" text-anchor="end" font-family="'DejaVu Sans Mono', monospace" font-size="17" fill="#63e6be">
    github.com/dotnetpower/agent-tokenomics
  </text>
</svg>`;

await sharp(Buffer.from(svg)).png({ compressionLevel: 9 }).toFile(OUT);
console.log(`[gen-og-image] wrote ${path.relative(process.cwd(), OUT)} (${WIDTH}x${HEIGHT})`);
