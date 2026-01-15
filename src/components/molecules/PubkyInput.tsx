import { useState, type ChangeEvent } from "react";
import { SearchIcon } from "@/components/atoms";
import { cn } from "@/lib/utils";

export interface PubkyInputProps {
  value: string;
  onChange: (value: string) => void;
  suggestions?: string[];
  placeholder?: string;
  className?: string;
}

export function PubkyInput({
  value,
  onChange,
  suggestions = [],
  placeholder = "Enter your pubky...",
  className,
}: PubkyInputProps) {
  const [isFocused, setIsFocused] = useState(false);
  const hasValue = value.trim().length > 0;

  const handleChange = (e: ChangeEvent<HTMLInputElement>) => {
    onChange(e.target.value);
  };

  return (
    <div
      className={cn(
        "relative flex items-center bg-black/10 border rounded-full px-5 py-4 gap-3 transition-colors duration-200",
        hasValue
          ? "border-solid border-border-dashed"
          : "border-dashed border-border-dashed",
        className,
      )}
    >
      <SearchIcon className="shrink-0 text-text-secondary" size={16} />
      <input
        type="text"
        value={value}
        onChange={handleChange}
        onFocus={() => setIsFocused(true)}
        onBlur={() => setIsFocused(false)}
        placeholder={isFocused ? "" : placeholder}
        list="previous-keys"
        autoComplete="off"
        className={cn(
          "flex-1 bg-transparent border-none outline-none text-base font-medium leading-normal p-0 m-0 transition-colors duration-200",
          hasValue ? "text-white" : "text-text-secondary",
          "placeholder:text-text-secondary",
        )}
      />
      {suggestions.length > 0 && (
        <datalist id="previous-keys">
          {suggestions.map((key) => (
            <option key={key} value={key} />
          ))}
        </datalist>
      )}
    </div>
  );
}
