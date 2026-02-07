import { createSignal, For, Show, onCleanup } from "solid-js";

interface Props {
  models: string[];
  current: string | null;
  onSelect: (model: string) => void;
  disabled?: boolean;
}

export default function ModelSelector(props: Props) {
  const [open, setOpen] = createSignal(false);
  let containerRef!: HTMLDivElement;

  function handleToggle() {
    if (props.disabled || props.models.length === 0) return;
    setOpen(!open());
  }

  function handleSelect(model: string) {
    if (model !== props.current) {
      props.onSelect(model);
    }
    setOpen(false);
  }

  function handleClickOutside(e: MouseEvent) {
    if (containerRef && !containerRef.contains(e.target as Node)) {
      setOpen(false);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      setOpen(false);
    }
  }

  // Attach/detach global listeners based on open state
  // Use createEffect-like pattern via event handlers
  function attachListeners() {
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
  }

  function detachListeners() {
    document.removeEventListener("mousedown", handleClickOutside);
    document.removeEventListener("keydown", handleKeyDown);
  }

  onCleanup(() => detachListeners());

  return (
    <div class="model-selector" ref={containerRef}>
      <Show
        when={props.models.length > 0}
        fallback={null}
      >
        <button
          class="model-selector-trigger"
          classList={{
            open: open(),
            disabled: props.disabled,
            switching: props.disabled,
          }}
          onClick={() => {
            handleToggle();
            if (open()) attachListeners();
            else detachListeners();
          }}
          disabled={props.disabled}
        >
          <Show
            when={!props.disabled}
            fallback={
              <span class="model-spinner" />
            }
          >
            <span class="model-name">
              {props.current || "default"}
            </span>
            <span class="model-chevron">&#9662;</span>
          </Show>
        </button>

        <Show when={open() && !props.disabled}>
          <div class="model-dropdown">
            <For each={props.models}>
              {(model) => (
                <button
                  class="model-option"
                  classList={{ selected: model === props.current }}
                  onClick={() => {
                    handleSelect(model);
                    detachListeners();
                  }}
                >
                  <span class="model-check">
                    {model === props.current ? "\u2713" : ""}
                  </span>
                  <span>{model}</span>
                </button>
              )}
            </For>
          </div>
        </Show>
      </Show>

      <style>{`
        .model-selector {
          position: relative;
          display: inline-flex;
        }
        .model-selector-trigger {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          padding: 2px 8px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: 10px;
          font-size: 11px;
          color: var(--text-secondary);
          cursor: pointer;
          transition: border-color 0.15s, background 0.15s;
        }
        .model-selector-trigger:hover:not(:disabled) {
          border-color: var(--accent);
          background: var(--bg-hover);
        }
        .model-selector-trigger.open {
          border-color: var(--accent);
        }
        .model-selector-trigger.disabled,
        .model-selector-trigger:disabled {
          cursor: default;
          opacity: 0.6;
        }
        .model-name {
          font-family: var(--font-mono);
          font-size: 11px;
        }
        .model-chevron {
          font-size: 8px;
          color: var(--text-muted);
        }
        .model-spinner {
          display: inline-block;
          width: 10px;
          height: 10px;
          border: 1.5px solid var(--text-muted);
          border-top-color: var(--accent);
          border-radius: 50%;
          animation: model-spin 0.6s linear infinite;
        }
        .model-dropdown {
          position: absolute;
          top: calc(100% + 4px);
          left: 0;
          min-width: 140px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: var(--radius);
          padding: 4px 0;
          box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
          z-index: 100;
        }
        .model-option {
          display: flex;
          align-items: center;
          gap: 8px;
          width: 100%;
          padding: 6px 12px;
          font-size: 12px;
          color: var(--text-primary);
          text-align: left;
        }
        .model-option:hover {
          background: var(--bg-hover);
        }
        .model-option.selected {
          color: var(--accent);
        }
        .model-check {
          width: 14px;
          font-size: 11px;
          color: var(--accent);
        }
        @keyframes model-spin {
          to { transform: rotate(360deg); }
        }
      `}</style>
    </div>
  );
}
