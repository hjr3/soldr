import { Show, SimpleShowLayout, TextField, Datagrid, ReferenceManyField } from 'react-admin';
import ConditionalDateField from '../ConditionalDateField';
import DateFieldSec from '../DateFieldSec';

export const RequestsShow = () => (
  <Show>
    <SimpleShowLayout>
      <TextField source="id" />
      <TextField source="method" />
      <TextField source="uri" />
      <TextField source="state" />
      <DateFieldSec source="created_at" label="Created At" showDate showTime />
      <ConditionalDateField
        source="retry_ms_at"
        label="Retry At"
        showDate
        showTime
        emptyText="Not scheduled"
      />
      <ReferenceManyField label="Attempts" reference="attempts" target="request_id" perPage={20}>
        <Datagrid rowClick="show" bulkActionButtons={false}>
          <TextField source="id" />
          <TextField source="response_status" />
          <DateFieldSec source="created_at" label="Created At" showDate showTime />
        </Datagrid>
      </ReferenceManyField>
    </SimpleShowLayout>
  </Show>
);

export default RequestsShow;
