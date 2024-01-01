import { Show, SimpleShowLayout, TextField } from 'react-admin';
import DateFieldSec from '../DateFieldSec';

export const AttemptsShow = () => (
  <Show>
    <SimpleShowLayout>
      <TextField source="id" />
      <TextField source="response_status" />
      <TextField source="response_body" component="pre" />
      <DateFieldSec source="created_at" label="Created At" showDate showTime />
    </SimpleShowLayout>
  </Show>
);

export default AttemptsShow;
