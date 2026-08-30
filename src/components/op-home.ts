import { LitElement, css, html } from 'lit';
import { loadWasm, type WasmInfo } from '../wasm';

interface StartingPoint {
  readonly name: string;
  readonly href: string;
  readonly note: string;
}

// A deliberately short list of canonical places to start. Sources and context
// for each entry are in docs/research/openpower-landscape.md.
const STARTING_POINTS: readonly StartingPoint[] = [
  {
    name: 'OpenPOWER Foundation',
    href: 'https://openpowerfoundation.org/',
    note: 'Power ISA specifications and working groups',
  },
  {
    name: 'Raptor Computing Systems wiki',
    href: 'https://wiki.raptorcs.com/wiki/Main_Page',
    note: 'Talos II and Blackbird documentation, firmware, compatibility lists',
  },
  {
    name: 'Raptor community forums',
    href: 'https://forums.raptorcs.com/',
    note: 'Operating systems, porting and hardware discussion',
  },
  {
    name: 'Talospace',
    href: 'https://www.talospace.com/',
    note: 'News on OpenPOWER desktops and ports',
  },
  {
    name: 'skiboot (OPAL)',
    href: 'https://github.com/open-power/skiboot',
    note: 'OpenPOWER boot and runtime firmware',
  },
  {
    name: 'op-build',
    href: 'https://github.com/open-power/op-build',
    note: 'Builds Hostboot, Skiboot, OCC and Petitboot into a PNOR image',
  },
  {
    name: 'OpenBMC',
    href: 'https://github.com/openbmc/openbmc',
    note: 'BMC firmware used on Talos II and Blackbird',
  },
  {
    name: '#talos-workstation',
    href: 'https://wiki.raptorcs.com/wiki/IRC',
    note: 'IRC on Libera.Chat, bridged to Matrix',
  },
];

// Written without decorators on purpose: `static properties` plus `declare`d
// fields is the plain form Lit documents for TypeScript, and it keeps the
// build independent of any decorator transform.
export class OpHome extends LitElement {
  static override properties = {
    wasm: { state: true },
    wasmError: { state: true },
  };

  static override styles = css`
    :host {
      display: block;
      max-width: 40rem;
      margin: 0 auto;
      padding: 2rem 1rem;
      font-family: system-ui, sans-serif;
      line-height: 1.5;
      color: #1a1a1a;
      background: #ffffff;
    }
    a {
      color: #0b57d0;
    }
    @media (prefers-color-scheme: dark) {
      :host {
        color: #e6e6e6;
        background: #121212;
      }
      a {
        color: #8ab4f8;
      }
    }
    h1 {
      font-size: 1.75rem;
      margin: 0 0 0.25rem;
    }
    h2 {
      font-size: 1.125rem;
      margin: 1.5rem 0 0.5rem;
    }
    p.tagline {
      margin: 0 0 1.5rem;
    }
    ul {
      padding-left: 1.25rem;
      margin: 0;
    }
    li {
      margin: 0.25rem 0;
    }
    footer {
      margin-top: 2rem;
      font-size: 0.875rem;
      opacity: 0.8;
    }
    code {
      font-family: ui-monospace, monospace;
    }
  `;

  declare wasm: WasmInfo | undefined;
  declare wasmError: string | undefined;

  constructor() {
    super();
    this.wasm = undefined;
    this.wasmError = undefined;
  }

  override connectedCallback(): void {
    super.connectedCallback();
    loadWasm().then(
      (info) => {
        this.wasm = info;
      },
      (err: unknown) => {
        this.wasmError = err instanceof Error ? err.message : String(err);
      },
    );
  }

  override render() {
    return html`
      <header>
        <h1>openpower.tools</h1>
        <p class="tagline">
          Community-driven support for OpenPOWER / Talos II firmware and software ports.
        </p>
      </header>
      <main>
        <p>This site is a skeleton; content is on its way.</p>
        <h2>Starting points</h2>
        <ul>
          ${STARTING_POINTS.map(
            (p) => html`<li><a href=${p.href}>${p.name}</a>: ${p.note}</li>`,
          )}
        </ul>
        <h2>Status</h2>
        <p>${this.renderWasmStatus()}</p>
      </main>
      <footer>
        <p>
          openpower.tools is an independent community project. It is not affiliated with or
          endorsed by the OpenPOWER Foundation, IBM, or Raptor Computing Systems.
        </p>
        <p>
          Source:
          <a href="https://github.com/openpower-tools/www.openpower.tools"
            >github.com/openpower-tools/www.openpower.tools</a
          >
        </p>
        <p>
          Code is licensed under the
          <a href="https://www.gnu.org/licenses/gpl-3.0.html">GPL-3.0-or-later</a>; content under
          <a href="https://creativecommons.org/licenses/by-sa/4.0/">CC BY-SA 4.0</a>. Product and
          organisation names belong to their owners. See
          <a href="https://github.com/openpower-tools/www.openpower.tools/blob/main/LICENSE.md"
            >LICENSE.md</a
          >.
        </p>
      </footer>
    `;
  }

  private renderWasmStatus() {
    if (this.wasmError !== undefined) {
      return html`WebAssembly module failed to load: <code>${this.wasmError}</code>`;
    }
    if (this.wasm === undefined) {
      return html`Loading WebAssembly module...`;
    }
    return html`<code>${this.wasm.statusLine}</code>`;
  }
}

customElements.define('op-home', OpHome);

declare global {
  interface HTMLElementTagNameMap {
    'op-home': OpHome;
  }
}
