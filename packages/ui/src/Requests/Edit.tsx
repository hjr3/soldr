import { Edit, SimpleForm, TextField, TextInput, required } from 'react-admin';
import HeadersDataGrid from './HeadersDataGrid';

export const RequestEdit = () => (
  <Edit>
    <SimpleForm>
      <TextField source="id" />
      <TextField source="state" />
      <TextInput source="method" validate={[required()]} />
      <TextInput source="uri" validate={[required()]} />
      <HeadersDataGrid source="headers" />
      <TextInput
        source="body"
        multiline
        fullWidth
        format={(v) => new TextDecoder().decode(new Uint8Array(v))}
        parse={(v) => Array.from(new TextEncoder().encode(v))}
      />
    </SimpleForm>
  </Edit>
);

export default RequestEdit;
