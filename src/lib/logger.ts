/**
 * Structured logging utility
 * Provides consistent logging with context and log levels
 */

type LogLevel = "debug" | "info" | "warn" | "error";

interface LogContext {
  [key: string]: unknown;
}

const LOG_LEVELS: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

// In production, only show warnings and errors
const MIN_LOG_LEVEL: LogLevel =
  import.meta.env.MODE === "production" ? "warn" : "debug";

function shouldLog(level: LogLevel): boolean {
  return LOG_LEVELS[level] >= LOG_LEVELS[MIN_LOG_LEVEL];
}

function formatMessage(tag: string, message: string): string {
  return `[${tag}] ${message}`;
}

export const Logger = {
  /**
   * Debug level - development diagnostics
   * @example Logger.debug('useFollowUser', 'Successfully followed user', { userId })
   */
  debug(tag: string, message: string, context?: LogContext): void {
    if (shouldLog("debug")) {
      if (context !== undefined) {
        console.log(formatMessage(tag, message), context);
      } else {
        console.log(formatMessage(tag, message));
      }
    }
  },

  /**
   * Info level - significant events
   * @example Logger.info('PostController', 'Post created successfully', { postId })
   */
  info(tag: string, message: string, context?: LogContext): void {
    if (shouldLog("info")) {
      if (context !== undefined) {
        console.info(formatMessage(tag, message), context);
      } else {
        console.info(formatMessage(tag, message));
      }
    }
  },

  /**
   * Warn level - potential issues
   * @example Logger.warn('ApiService', 'Rate limit approaching', { remaining: 10 })
   */
  warn(tag: string, message: string, context?: LogContext): void {
    if (shouldLog("warn")) {
      if (context !== undefined) {
        console.warn(formatMessage(tag, message), context);
      } else {
        console.warn(formatMessage(tag, message));
      }
    }
  },

  /**
   * Error level - failures requiring attention
   * @example Logger.error('useBookmark', 'Failed to toggle bookmark', { error, postId })
   */
  error(tag: string, message: string, context?: LogContext): void {
    if (shouldLog("error")) {
      if (context !== undefined) {
        console.error(formatMessage(tag, message), context);
      } else {
        console.error(formatMessage(tag, message));
      }
    }
  },
} as const;
