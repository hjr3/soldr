import { Show, SimpleShowLayout, TextField } from 'react-admin';
import DateFieldSec from '../DateFieldSec';
import Uint8ArrayField from '../Uint8ArrayField';

export const AttemptsShow = () => (
  <Show>
    <SimpleShowLayout>
      <TextField source="id" />
      <TextField source="response_status" />
      <Uint8ArrayField source="response_body" />
      <DateFieldSec source="created_at" label="Created At" showDate showTime />
    </SimpleShowLayout>
  </Show>
);

export default AttemptsShow;
