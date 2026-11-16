import type { BrowserOptions } from "@zuruf/protocol";
import { sessionModeLabel } from "../lib/text";

const INHERIT = "";

/**
 * How sessions open for a project or a start option. Every choice can stay on "inherit":
 * a start option then follows its project, a project follows the app settings.
 */
export function BrowserOptionFields(props: {
  value: BrowserOptions;
  inheritLabel: string;
  onChange: (next: BrowserOptions) => void;
}) {
  const set = (patch: BrowserOptions) => props.onChange({ ...props.value, ...patch });
  const agentAccess = () => (props.value.agentAccess == null ? INHERIT : props.value.agentAccess ? "yes" : "no");

  return (
    <div class="fields fields-3">
      <label class="labeled">
        <span>Sitzung</span>
        <select
          class="input"
          value={props.value.mode ?? INHERIT}
          onChange={(event) => set({ mode: (event.currentTarget.value || null) as BrowserOptions["mode"] })}
        >
          <option value={INHERIT}>{props.inheritLabel}</option>
          <option value="shared">{sessionModeLabel.shared}</option>
          <option value="isolated">{sessionModeLabel.isolated}</option>
        </select>
      </label>
      <label class="labeled">
        <span>Agenten</span>
        <select
          class="input"
          value={agentAccess()}
          onChange={(event) => {
            const picked = event.currentTarget.value;
            set({ agentAccess: picked === INHERIT ? null : picked === "yes" });
          }}
        >
          <option value={INHERIT}>{props.inheritLabel}</option>
          <option value="yes">dürfen steuern</option>
          <option value="no">dürfen nicht steuern</option>
        </select>
      </label>
      <label class="labeled">
        <span>Fenster</span>
        <select
          class="input"
          value={props.value.window ?? INHERIT}
          onChange={(event) => set({ window: (event.currentTarget.value || null) as BrowserOptions["window"] })}
        >
          <option value={INHERIT}>{props.inheritLabel}</option>
          <option value="integrated">Ein Fenster</option>
          <option value="separate">Getrennt</option>
        </select>
      </label>
    </div>
  );
}

/** True when two option sets mean the same, treating missing and null alike. */
export function sameBrowserOptions(a: BrowserOptions, b: BrowserOptions): boolean {
  return (
    (a.mode ?? null) === (b.mode ?? null) &&
    (a.agentAccess ?? null) === (b.agentAccess ?? null) &&
    (a.window ?? null) === (b.window ?? null)
  );
}
