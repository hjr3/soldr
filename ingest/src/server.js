import express from 'express';
import bodyParser from 'body-parser';
import pg from 'pg';
import { createHttpTerminator } from 'http-terminator';

import { initLogger } from './logger.js';

export default function server() {
  const { Pool } = pg;
  const pool = new Pool();
  const logger = initLogger();

  const app = express();

  app.use(
    bodyParser.raw({
      inflate: true,
      limit: '100kb',
      type: '*/*',
    }),
  );

  //app.use((req, _res, next) => {
  //  req.pgPool = pool;
  //  req.logger = logger;
  //  next();
  //});

  app.all('*', (req, res) => {
    const values = [
      req.method,
      req.protocol,
      req.hostname,
      req.originalUrl,
      // FIXME use raw headers
      req.headers,
      req.body.toString(),
    ];

    const q =
      'INSERT INTO requests (method, protocol, hostname, url, headers, body) VALUES($1, $2, $3, $4, $5, $6)';

    pool
      .query(q, values)
      .catch((err) => {
        logger.error({ values, err }, 'Failed to save request');
      })
      .finally(() => {
        res.sendStatus(200);
      });
  });

  let expressServer;
  async function startServer({ port }) {
    return new Promise((resolve) => {
      const server = app.listen(port, () => resolve({ server, stopServer }));
      const httpTerminator = createHttpTerminator({ server });
      function stopServer() {
        return httpTerminator.terminate();
      }
    });
  }

  async function start({ port }) {
    expressServer = await startServer({ port });
    logger.info(`Ingest server listening on port ${port}`);
  }

  async function stop() {
    if (expressServer) {
      await expressServer.stopServer();
      logger.info('express stopped');
    }

    await pool.end();
    logger.info('pool stopped');
  }

  return {
    logger,
    start,
    stop,
  };
}
