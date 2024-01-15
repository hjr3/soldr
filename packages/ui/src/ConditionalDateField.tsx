import { TextField, DateField, DateFieldProps, useRecordContext } from 'react-admin';

interface Props extends DateFieldProps {
  source: string;
  emptyText: string;
}

const ConditionalDateField = (props: Props) => {
  const record = useRecordContext();
  return record && record[props.source] ? (
    <DateField {...props} />
  ) : (
    <TextField source="foo" emptyText={props.emptyText} />
  );
};

export default ConditionalDateField;
