import { List, Datagrid, TextField } from 'react-admin';
import DateFieldSec from '../DateFieldSec';

export const OriginsList = () => (
  <List>
    <Datagrid rowClick="edit" bulkActionButtons={false}>
      <TextField source="id" />
      <TextField source="domain" />
      <TextField source="origin_uri" label="Origin URI" />
      <DateFieldSec source="created_at" label="Created At" />
      <DateFieldSec source="updated_at" label="Updated At" />
    </Datagrid>
  </List>
);

export default OriginsList;
