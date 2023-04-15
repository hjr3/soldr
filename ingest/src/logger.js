import pino from 'pino';

const severities = {
  trace: 'DEBUG',
  debug: 'DEBUG',
  info: 'INFO',
  warn: 'WARNING',
  error: 'ERROR',
  fatal: 'CRITICAL',
};

export function initLogger() {
  return pino({
    level: 'info',
    timestamp: false, // prevents `timestamp` property
    messageKey: 'message',
    formatters: {
      bindings: () => ({}), // prevents `hostname` and `pid` properties from being logged
      level(label, number) {
        return {
          severity: severities[label] || 'INFO',
          level: number,
        };
      },
    },
    serializers: {
      err: serializeError,
    },
  });
}

function serializeError(err) {
  const serialized = pino.stdSerializers.err(err);
  if (Array.isArray(serialized.errors)) {
    serialized.errors = serialized.errors.map((e) => {
      if (e instanceof Error) {
        return serializeError(e);
      }
      return e;
    });
  }
  return serialized;
}
