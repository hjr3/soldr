import ReplayIcon from '@mui/icons-material/Replay';
import { useState } from 'react';
import {
  Button,
  Confirm,
  useListContext,
  useCreate,
  useNotify,
  useRefresh,
  useUnselectAll,
} from 'react-admin';

const RetryButton = () => {
  const { selectedIds } = useListContext();
  const [open, setOpen] = useState(false);
  const refresh = useRefresh();
  const notify = useNotify();
  const unselectAll = useUnselectAll('posts');
  const [create, { isLoading }] = useCreate();
  const handleClick = () => setOpen(true);
  const handleDialogClose = () => setOpen(false);

  const handleConfirm = () => {
    Promise.allSettled(
      selectedIds.map((requestId) =>
        create('queue', { data: { req_id: requestId } }, { returnPromise: true }),
      ),
    )
      .then(() => {
        refresh();
        notify('Requests added to retry queue');
        unselectAll();
      })
      .catch(() => notify('Error: requests not retried', { type: 'error' }))
      .finally(() => setOpen(false));
  };

  return (
    <>
      <Button label="Retry Requests" onClick={handleClick}>
        <ReplayIcon />
      </Button>
      <Confirm
        isOpen={open}
        loading={isLoading}
        title="Retry Requests"
        content="Are you sure you want to retry the selected requests?"
        onConfirm={handleConfirm}
        onClose={handleDialogClose}
      />
    </>
  );
};

export default RetryButton;
