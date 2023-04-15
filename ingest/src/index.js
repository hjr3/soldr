import server from './server.js';

const app = server();
const port = +(process.env.PORT || 3000);

const handleError = (err) => {
  app.logger.error({ err }, 'Uncaught Exception');
  process.exit(1);
};

process.on('uncaughtException', handleError);
process.on('unhandledRejection', handleError);

process.on('SIGTERM', () => {
  app
    .stop()
    .then(() => process.exit(0))
    .catch(handleError);
});

app
  .start({
    port,
  })
  .catch(handleError);
