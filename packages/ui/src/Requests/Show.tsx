import {
  Button,
  Show,
  SimpleShowLayout,
  TextField,
  Datagrid,
  ReferenceManyField,
  useCreate,
  useNotify,
  useShowContext,
} from 'react-admin';
import ReplayIcon from '@mui/icons-material/Replay';
import ConditionalDateField from '../ConditionalDateField';
import DateFieldSec from '../DateFieldSec';
import { EditButton, TopToolbar } from 'react-admin';

const RequestShowActions = () => {
  const notify = useNotify();
  const { record } = useShowContext();
  const [create, { isLoading }] = useCreate();

  const handleClick = () => {
    create('queue', { data: { req_id: record.id } }, { returnPromise: true })
      .then(() => {
        notify('Requests added to retry queue');
      })
      .catch(() => notify('Error: requests not retried', { type: 'error' }));
  };

  return (
    <TopToolbar>
      <EditButton />
      <Button color="primary" label="Retry Requests" disabled={isLoading} onClick={handleClick}>
        <ReplayIcon />
      </Button>
    </TopToolbar>
  );
};

export const RequestsShow = () => (
  <Show actions={<RequestShowActions />}>
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
