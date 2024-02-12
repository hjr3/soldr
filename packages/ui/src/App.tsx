import { Admin, Resource, fetchUtils } from 'react-admin';
import simpleRestProvider from 'ra-data-simple-rest';

import Layout from './Layout';
import Dashboard from './Dashboard';
import Origins from './Origins';
import Requests from './Requests';
import Attempts from './Attempts';

declare global {
  interface Window {
    config: {
      apiUrl?: string;
      apiSecret?: string;
    };
  }
}

const config = import.meta.env.PROD
  ? window.config
  : {
      apiUrl: import.meta.env.VITE_MGMT_API_URL,
      apiSecret: import.meta.env.VITE_MGMT_API_SECRET,
    };

if (!config.apiUrl) {
  throw new Error('API URL is required');
}

const httpClient = (url: string, options: fetchUtils.Options = {}) => {
  const user = { token: `Basic ${btoa(config.apiSecret)}`, authenticated: true };
  return fetchUtils.fetchJson(url, { ...options, user });
};

const dataProvider = simpleRestProvider(config.apiUrl, httpClient);

const App = () => (
  <Admin dataProvider={dataProvider} layout={Layout} dashboard={Dashboard}>
    <Resource name="origins" {...Origins} />
    <Resource name="requests" {...Requests} />
    <Resource name="attempts" {...Attempts} />
    <Resource name="queue" />
  </Admin>
);

export default App;
