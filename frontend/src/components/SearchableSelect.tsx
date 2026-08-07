import { useEffect, useMemo, useState } from "react";

export interface SearchableOption {
  value: string;
  label: string;
}

interface SearchableSelectProps {
  value: string;
  options: SearchableOption[];
  onChange: (value: string) => void;
  placeholder: string;
  emptyLabel?: string;
  ariaLabel?: string;
  disabled?: boolean;
  required?: boolean;
  className?: string;
}

export function SearchableSelect({
  value,
  options,
  onChange,
  placeholder,
  emptyLabel,
  ariaLabel,
  disabled = false,
  required = false,
  className = "",
}: SearchableSelectProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const selected = options.find((option) => option.value === value);

  useEffect(() => {
    if (!open) setQuery(selected?.label ?? "");
  }, [open, selected?.label]);

  const visible = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    if (!keyword || selected?.label === query) return options;
    return options.filter((option) =>
      option.label.toLowerCase().includes(keyword),
    );
  }, [options, query, selected?.label]);

  function choose(option?: SearchableOption) {
    onChange(option?.value ?? "");
    setQuery(option?.label ?? "");
    setOpen(false);
  }

  return (
    <div
      className={`searchable-select ${open ? "open" : ""} ${className}`.trim()}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node)) {
          setOpen(false);
          setQuery(selected?.label ?? "");
        }
      }}
    >
      <div className="searchable-select-control">
        <span aria-hidden="true">⌕</span>
        <input
          role="combobox"
          aria-label={ariaLabel ?? placeholder}
          aria-expanded={open}
          autoComplete="off"
          disabled={disabled}
          required={required && !value}
          placeholder={placeholder}
          value={query}
          onFocus={() => {
            setOpen(true);
            if (selected) setQuery("");
          }}
          onChange={(event) => {
            setQuery(event.target.value);
            setOpen(true);
            if (value) onChange("");
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape") setOpen(false);
            if (event.key === "Enter" && open && visible.length === 1) {
              event.preventDefault();
              choose(visible[0]);
            }
          }}
        />
        <button
          type="button"
          aria-label={`展开${ariaLabel ?? "选项"}`}
          disabled={disabled}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => setOpen((current) => !current)}
        >
          ▾
        </button>
      </div>
      {open && !disabled && (
        <div className="searchable-select-menu">
          {emptyLabel && (
            <button
              type="button"
              className={!value ? "selected" : ""}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose()}
            >
              {emptyLabel}
            </button>
          )}
          {visible.map((option) => (
            <button
              type="button"
              className={option.value === value ? "selected" : ""}
              key={option.value}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(option)}
            >
              {option.label}
            </button>
          ))}
          {visible.length === 0 && (
            <span className="searchable-select-empty">没有匹配项</span>
          )}
        </div>
      )}
    </div>
  );
}
