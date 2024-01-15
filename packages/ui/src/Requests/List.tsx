import { List, Datagrid, TextField, SelectArrayInput } from 'react-admin';
import ConditionalDateField from '../ConditionalDateField';
import DateFieldSec from '../DateFieldSec';

const filters = [
  <SelectArrayInput
    source="state"
    choices={[
      { id: '0', name: 'Received' },
      { id: '1', name: 'Created' },
      { id: '2', name: 'Enqueued' },
      { id: '3', name: 'Active' },
      { id: '4', name: 'Completed' },
      { id: '5', name: 'Failed' },
      { id: '6', name: 'Panic' },
      { id: '7', name: 'Timeout' },
      { id: '8', name: 'Skipped' },
    ]}
    parse={(values: string[]) => values.map((v) => parseInt(v))}
    alwaysOn
  />,
];

export const RequestsList = () => (
  <List filters={filters}>
    <Datagrid rowClick="show" bulkActionButtons={false}>
      <TextField source="id" />
      <TextField source="method" />
      <TextField source="uri" />
      <TextField source="state" />
      <DateFieldSec source="created_at" label="Created At" />
      <ConditionalDateField
        source="retry_ms_at"
        label="Retry At"
        showDate
        showTime
        emptyText="Not scheduled"
      />
    </Datagrid>
  </List>
);

export default RequestsList;
