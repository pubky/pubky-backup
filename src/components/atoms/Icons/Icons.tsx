import { cn } from "@/lib/utils";

export interface IconProps {
  className?: string;
  size?: number;
}

export function SearchIcon({ className, size = 16 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M7.33333 12.6667C10.2789 12.6667 12.6667 10.2789 12.6667 7.33333C12.6667 4.38781 10.2789 2 7.33333 2C4.38781 2 2 4.38781 2 7.33333C2 10.2789 4.38781 12.6667 7.33333 12.6667Z"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M14 14L11.1 11.1"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function BackIcon({ className, size = 20 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M12.5 15L7.5 10L12.5 5"
        stroke="currentColor"
        strokeWidth="1.67"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function CopyIcon({ className, size = 20 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M16.6667 7.5H9.16667C8.24619 7.5 7.5 8.24619 7.5 9.16667V16.6667C7.5 17.5871 8.24619 18.3333 9.16667 18.3333H16.6667C17.5871 18.3333 18.3333 17.5871 18.3333 16.6667V9.16667C18.3333 8.24619 17.5871 7.5 16.6667 7.5Z"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M4.16667 12.5H3.33333C2.89131 12.5 2.46738 12.3244 2.15482 12.0118C1.84226 11.6993 1.66667 11.2754 1.66667 10.8333V3.33333C1.66667 2.89131 1.84226 2.46738 2.15482 2.15482C2.46738 1.84226 2.89131 1.66667 3.33333 1.66667H10.8333C11.2754 1.66667 11.6993 1.84226 12.0118 2.15482C12.3244 2.46738 12.5 2.89131 12.5 3.33333V4.16667"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function RefreshIcon({ className, size = 24 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M3 12C3 9.61305 3.94821 7.32387 5.63604 5.63604C7.32387 3.94821 9.61305 3 12 3C14.516 3.00947 16.931 3.99122 18.74 5.74L21 8M21 8V3M21 8H16M21 12C21 14.3869 20.0518 16.6761 18.364 18.364C16.6761 20.0518 14.3869 21 12 21C9.48395 20.9905 7.06897 20.0088 5.26 18.26L3 16M3 16H8M3 16V21"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function CheckIcon({ className, size = 24 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M22 11.08V12C21.9988 14.1564 21.3005 16.2547 20.0093 17.9818C18.7182 19.7088 16.9033 20.9725 14.8354 21.5839C12.7674 22.1953 10.5573 22.1219 8.53447 21.3746C6.51168 20.6273 4.78465 19.2461 3.61096 17.4371C2.43727 15.628 1.87979 13.4881 2.02168 11.3363C2.16356 9.18455 2.99721 7.13631 4.39828 5.49706C5.79935 3.85781 7.69279 2.71537 9.79619 2.24013C11.8996 1.7649 14.1003 1.98232 16.07 2.85999"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M22 4L12 14.01L9 11.01"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function ExternalLinkIcon({ className, size = 24 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M15 3H21M21 3V9M21 3L10 14M18 13V19C18 19.5304 17.7893 20.0391 17.4142 20.4142C17.0391 20.7893 16.5304 21 16 21H5C4.46957 21 3.96086 20.7893 3.58579 20.4142C3.21071 20.0391 3 19.5304 3 19V8C3 7.46957 3.21071 6.96086 3.58579 6.58579C3.96086 6.21071 4.46957 6 5 6H11"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function DatabaseIcon({ className, size = 24 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M21 5C21 6.65685 16.9706 8 12 8C7.02944 8 3 6.65685 3 5M21 5C21 3.34315 16.9706 2 12 2C7.02944 2 3 3.34315 3 5M21 5V19C21 19.7956 20.0518 20.5587 18.364 21.1213C16.6761 21.6839 14.3869 22 12 22C9.61305 22 7.32387 21.6839 5.63604 21.1213C3.94821 20.5587 3 19.7956 3 19V5M3 12C3 12.7956 3.94821 13.5587 5.63604 14.1213C7.32387 14.6839 9.61305 15 12 15C14.3869 15 16.6761 14.6839 18.364 14.1213C20.0518 13.5587 21 12.7956 21 12"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function ClockIcon({ className, size = 24 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M12 6V12L16 14M22 12C22 17.5228 17.5228 22 12 22C6.47715 22 2 17.5228 2 12C2 6.47715 6.47715 2 12 2C17.5228 2 22 6.47715 22 12Z"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function FolderIcon({ className, size = 24 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M20 20C20.5304 20 21.0391 19.7893 21.4142 19.4142C21.7893 19.0391 22 18.5304 22 18V8C22 7.46957 21.7893 6.96086 21.4142 6.58579C21.0391 6.21071 20.5304 6 20 6H12.1C11.7655 6.00328 11.4355 5.92261 11.1403 5.76538C10.8451 5.60815 10.594 5.37938 10.41 5.1L9.6 3.9C9.41789 3.62347 9.16997 3.39648 8.8785 3.2394C8.58702 3.08231 8.26111 3.00005 7.93 3H4C3.46957 3 2.96086 3.21071 2.58579 3.58579C2.21071 3.96086 2 4.46957 2 5V18C2 18.5304 2.21071 19.0391 2.58579 19.4142C2.96086 19.7893 3.46957 20 4 20H20Z"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function SnapshotIcon({ className, size = 24 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M21 5C21 6.65685 16.9706 8 12 8C7.02944 8 3 6.65685 3 5M21 5C21 3.34315 16.9706 2 12 2C7.02944 2 3 3.34315 3 5M21 5V19C21 19.7956 20.0518 20.5587 18.364 21.1213C16.6761 21.6839 14.3869 22 12 22C9.61305 22 7.32387 21.6839 5.63604 21.1213C3.94821 20.5587 3 19.7956 3 19V5M3 12C3 12.7956 3.94821 13.5587 5.63604 14.1213C7.32387 14.6839 9.61305 15 12 15C14.3869 15 16.6761 14.6839 18.364 14.1213C20.0518 13.5587 21 12.7956 21 12M13 10.5V18.25M16 14.75L13 11.75L10 14.75"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

// Navigation icons
export function NavSyncIcon({ className, size = 20 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M2.5 10C2.5 8.01088 3.29018 6.10322 4.6967 4.6967C6.10322 3.29018 8.01088 2.5 10 2.5C12.0967 2.50789 14.1092 3.32602 15.6167 4.78333L17.5 6.66667M17.5 6.66667V2.5M17.5 6.66667H13.3333M17.5 10C17.5 11.9891 16.7098 13.8968 15.3033 15.3033C13.8968 16.7098 11.9891 17.5 10 17.5C7.90329 17.4921 5.89081 16.674 4.38333 15.2167L2.5 13.3333M2.5 13.3333H6.66667M2.5 13.3333V17.5"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function NavActivityIcon({ className, size = 20 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M18.3337 10H16.267C15.9028 9.99924 15.5484 10.1178 15.2579 10.3375C14.9675 10.5572 14.757 10.866 14.6587 11.2167L12.7003 18.1834C12.6877 18.2266 12.6614 18.2646 12.6253 18.2917C12.5893 18.3187 12.5454 18.3334 12.5003 18.3334C12.4552 18.3334 12.4114 18.3187 12.3753 18.2917C12.3393 18.2646 12.3129 18.2266 12.3003 18.1834L7.70033 1.81669C7.6877 1.77341 7.66139 1.7354 7.62533 1.70835C7.58926 1.68131 7.5454 1.66669 7.50033 1.66669C7.45525 1.66669 7.41139 1.68131 7.37533 1.70835C7.33926 1.7354 7.31295 1.77341 7.30033 1.81669L5.34199 8.78335C5.24405 9.13265 5.03481 9.44045 4.74605 9.66003C4.45728 9.87961 4.10476 9.99898 3.74199 10H1.66699"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function NavKeysIcon({ className, size = 20 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M12.917 6.25002L14.8337 8.16669C14.9894 8.31938 15.1989 8.4049 15.417 8.4049C15.6351 8.4049 15.8446 8.31938 16.0003 8.16669L17.7503 6.41669C17.903 6.26091 17.9885 6.05148 17.9885 5.83335C17.9885 5.61523 17.903 5.40579 17.7503 5.25002L15.8337 3.33335M17.5002 1.66669L9.50024 9.66669M10.8337 12.9167C10.8337 15.448 8.78163 17.5 6.25033 17.5C3.71902 17.5 1.66699 15.448 1.66699 12.9167C1.66699 10.3854 3.71902 8.33335 6.25033 8.33335C8.78163 8.33335 10.8337 10.3854 10.8337 12.9167Z"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function NavSettingsIcon({ className, size = 20 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M10.1833 1.66669H9.81667C9.37464 1.66669 8.95072 1.84228 8.63816 2.15484C8.3256 2.4674 8.15 2.89133 8.15 3.33335V3.48335C8.1497 3.77562 8.07255 4.06268 7.92628 4.31572C7.78002 4.56876 7.56978 4.77888 7.31667 4.92502L6.95834 5.13335C6.70497 5.27963 6.41756 5.35664 6.125 5.35664C5.83244 5.35664 5.54503 5.27963 5.29167 5.13335L5.16667 5.06669C4.78422 4.84607 4.32987 4.78622 3.90334 4.90028C3.47681 5.01433 3.11296 5.29297 2.89167 5.67502L2.70833 5.99169C2.48772 6.37413 2.42787 6.82849 2.54192 7.25502C2.65598 7.68155 2.93461 8.04539 3.31667 8.26669L3.44167 8.35002C3.69356 8.49545 3.90302 8.70426 4.04921 8.95571C4.1954 9.20716 4.27325 9.4925 4.275 9.78335V10.2084C4.27617 10.502 4.19971 10.7908 4.05337 11.0454C3.90703 11.3001 3.69601 11.5115 3.44167 11.6584L3.31667 11.7334C2.93461 11.9546 2.65598 12.3185 2.54192 12.745C2.42787 13.1716 2.48772 13.6259 2.70833 14.0084L2.89167 14.325C3.11296 14.7071 3.47681 14.9857 3.90334 15.0998C4.32987 15.2138 4.78422 15.154 5.16667 14.9334L5.29167 14.8667C5.54503 14.7204 5.83244 14.6434 6.125 14.6434C6.41756 14.6434 6.70497 14.7204 6.95834 14.8667L7.31667 15.075C7.56978 15.2212 7.78002 15.4313 7.92628 15.6843C8.07255 15.9374 8.1497 16.2244 8.15 16.5167V16.6667C8.15 17.1087 8.3256 17.5326 8.63816 17.8452C8.95072 18.1578 9.37464 18.3334 9.81667 18.3334H10.1833C10.6254 18.3334 11.0493 18.1578 11.3618 17.8452C11.6744 17.5326 11.85 17.1087 11.85 16.6667V16.5167C11.8503 16.2244 11.9275 15.9374 12.0737 15.6843C12.22 15.4313 12.4302 15.2212 12.6833 15.075L13.0417 14.8667C13.295 14.7204 13.5824 14.6434 13.875 14.6434C14.1676 14.6434 14.455 14.7204 14.7083 14.8667L14.8333 14.9334C15.2158 15.154 15.6701 15.2138 16.0967 15.0998C16.5232 14.9857 16.887 14.7071 17.1083 14.325L17.2917 14C17.5123 13.6176 17.5721 13.1632 17.4581 12.7367C17.344 12.3102 17.0654 11.9463 16.6833 11.725L16.5583 11.6584C16.304 11.5115 16.093 11.3001 15.9466 11.0454C15.8003 10.7908 15.7238 10.502 15.725 10.2084V9.79169C15.7238 9.498 15.8003 9.20923 15.9466 8.9546C16.093 8.69997 16.304 8.48853 16.5583 8.34169L16.6833 8.26669C17.0654 8.04539 17.344 7.68155 17.4581 7.25502C17.5721 6.82849 17.5123 6.37413 17.2917 5.99169L17.1083 5.67502C16.887 5.29297 16.5232 5.01433 16.0967 4.90028C15.6701 4.78622 15.2158 4.84607 14.8333 5.06669L14.7083 5.13335C14.455 5.27963 14.1676 5.35664 13.875 5.35664C13.5824 5.35664 13.295 5.27963 13.0417 5.13335L12.6833 4.92502C12.4302 4.77888 12.22 4.56876 12.0737 4.31572C11.9275 4.06268 11.8503 3.77562 11.85 3.48335V3.33335C11.85 2.89133 11.6744 2.4674 11.3618 2.15484C11.0493 1.84228 10.6254 1.66669 10.1833 1.66669Z"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M10 12.5C11.3807 12.5 12.5 11.3807 12.5 10C12.5 8.61931 11.3807 7.50002 10 7.50002C8.61929 7.50002 7.5 8.61931 7.5 10C7.5 11.3807 8.61929 12.5 10 12.5Z"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function ChevronDownIcon({ className, size = 16 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M4 6L8 10L12 6"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function TrashIcon({ className, size = 20 }: IconProps) {
  return (
    <svg
      className={cn("shrink-0", className)}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M2.5 5.00002H17.5M15.8333 5.00002V16.6667C15.8333 17.5 15 18.3334 14.1667 18.3334H5.83333C5 18.3334 4.16667 17.5 4.16667 16.6667V5.00002M6.66667 5.00002V3.33335C6.66667 2.50002 7.5 1.66669 8.33333 1.66669H11.6667C12.5 1.66669 13.3333 2.50002 13.3333 3.33335V5.00002"
        stroke="currentColor"
        strokeWidth="1.33"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
