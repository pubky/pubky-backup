import { useState, useRef, useEffect, useCallback } from "react";
import * as Atoms from "@/components/atoms";
import * as Utils from "@/utils";
import { cn } from "@/lib";

interface KeyOption {
  pubky: string;
  isSelected: boolean;
}

export interface DashboardHeaderProps {
  pubkyDisplay: string;
  status: Atoms.StatusBadgeProps["status"];
  onCopy: () => void;
  keys?: KeyOption[];
  onSelectKey?: (pubky: string) => void;
}

export function DashboardHeader({
  pubkyDisplay,
  status,
  onCopy,
  keys = [],
  onSelectKey,
}: DashboardHeaderProps) {
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const optionRefs = useRef<(HTMLButtonElement | null)[]>([]);

  // Find initially selected index
  const selectedIndex = keys.findIndex((k) => k.isSelected);

  // Close dropdown when clicking outside
  useEffect(() => {
    if (!isDropdownOpen) {
      return;
    }

    function handleClickOutside(event: MouseEvent) {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsDropdownOpen(false);
      }
    }

    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [isDropdownOpen]);

  // Close dropdown on Escape key
  useEffect(() => {
    if (!isDropdownOpen) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setIsDropdownOpen(false);
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [isDropdownOpen]);

  // Focus management when dropdown opens
  useEffect(() => {
    if (isDropdownOpen) {
      const initialIndex = selectedIndex >= 0 ? selectedIndex : 0;
      setFocusedIndex(initialIndex);
      optionRefs.current[initialIndex]?.focus();
    } else {
      setFocusedIndex(-1);
    }
  }, [isDropdownOpen, selectedIndex]);

  const handleSelectKey = useCallback(
    (pubky: string) => {
      onSelectKey?.(pubky);
      setIsDropdownOpen(false);
    },
    [onSelectKey],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (!isDropdownOpen || keys.length === 0) return;

      switch (event.key) {
        case "ArrowDown": {
          event.preventDefault();
          const nextIndex = (focusedIndex + 1) % keys.length;
          setFocusedIndex(nextIndex);
          optionRefs.current[nextIndex]?.focus();
          break;
        }
        case "ArrowUp": {
          event.preventDefault();
          const prevIndex = (focusedIndex - 1 + keys.length) % keys.length;
          setFocusedIndex(prevIndex);
          optionRefs.current[prevIndex]?.focus();
          break;
        }
        case "Enter": {
          event.preventDefault();
          const key = keys[focusedIndex];
          if (focusedIndex >= 0 && focusedIndex < keys.length && key) {
            handleSelectKey(key.pubky);
          }
          break;
        }
      }
    },
    [isDropdownOpen, keys, focusedIndex, handleSelectKey],
  );

  const showDropdown = keys.length > 1;

  return (
    <div className="flex items-center self-stretch gap-1.5">
      <div
        className="flex items-center gap-1.5 flex-1 relative"
        ref={dropdownRef}
      >
        <h4 className="text-base font-bold text-white m-0">{pubkyDisplay}</h4>
        <Atoms.IconButton
          variant="inline"
          onClick={onCopy}
          title="Copy full pubky"
        >
          <Atoms.CopyIcon size={16} />
        </Atoms.IconButton>
        {showDropdown && (
          <>
            <Atoms.IconButton
              variant="inline"
              onClick={() => setIsDropdownOpen(!isDropdownOpen)}
              title="Switch key"
              aria-expanded={isDropdownOpen}
              aria-haspopup="listbox"
            >
              <Atoms.ChevronDownIcon
                size={16}
                className={cn(
                  "transition-transform duration-200",
                  isDropdownOpen && "rotate-180",
                )}
              />
            </Atoms.IconButton>
            {isDropdownOpen && (
              <div
                role="listbox"
                aria-label="Select key"
                tabIndex={-1}
                onKeyDown={handleKeyDown}
                className="absolute top-full left-0 mt-2 z-50 min-w-[200px] bg-surface-dark border border-border rounded-lg shadow-lg overflow-hidden"
              >
                {keys.map((key, index) => (
                  <button
                    key={key.pubky}
                    ref={(el) => {
                      optionRefs.current[index] = el;
                    }}
                    type="button"
                    role="option"
                    aria-selected={key.isSelected}
                    onClick={() => handleSelectKey(key.pubky)}
                    className={cn(
                      "flex items-center gap-2 w-full px-3 py-2.5 text-left",
                      "hover:bg-surface-light transition-colors duration-150",
                      "focus:bg-surface-light focus:outline-none",
                      key.isSelected && "bg-surface-light",
                    )}
                  >
                    <Atoms.NavKeysIcon
                      size={16}
                      className="text-text-secondary shrink-0"
                    />
                    <span className="text-sm text-white font-medium">
                      {Utils.displayPubky(key.pubky)}
                    </span>
                    {key.isSelected && (
                      <Atoms.CheckIcon
                        size={14}
                        className="text-pubky-lime ml-auto shrink-0"
                      />
                    )}
                  </button>
                ))}
              </div>
            )}
          </>
        )}
      </div>
      <Atoms.StatusBadge status={status} />
    </div>
  );
}
