import { FieldProps, useRecordContext } from 'react-admin';
import Typography from '@mui/material/Typography';

// Convert a UTF8 array into a string
const Uint8ArrayField = (props: FieldProps) => {
  const record = useRecordContext(props);
  if (!record) {
    return null;
  }

  if (!props.source) {
    return null;
  }

  const arr = new Uint8Array(record[props.source]);
  const str = new TextDecoder().decode(arr);

  return (
    <Typography component="pre" variant="body2">
      {str}
    </Typography>
  );
};

export default Uint8ArrayField;
