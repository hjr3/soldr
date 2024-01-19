import { Admin, Resource } from 'react-admin';
import simpleRestProvider from 'ra-data-simple-rest';

import Layout from './Layout';
import Dashboard from './Dashboard';
import Origins from './Origins';
import Requests from './Requests';
import Attempts from './Attempts';

declare global {
  interface Window {
    apiUrl?: string;
  }
}

const config = {
  apiUrl: import.meta.env.PROD ? window.apiUrl : import.meta.env.VITE_MGMT_API_URL,
};

if (!config.apiUrl) {
  throw new Error('API URL is required');
}

const dataProvider = simpleRestProvider(config.apiUrl);

const App = () => (
  <Admin dataProvider={dataProvider} layout={Layout} dashboard={Dashboard}>
    <Resource name="origins" {...Origins} />
    <Resource name="requests" {...Requests} />
    <Resource name="attempts" {...Attempts} />
    <Resource name="queue" />
  </Admin>
);

export default App;
