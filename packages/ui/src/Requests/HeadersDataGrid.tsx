import * as React from 'react';
import Box from '@mui/material/Box';
import {
  GridRowsProp,
  GridRowModesModel,
  DataGrid,
  GridColDef,
  GridEventListener,
  GridRowModel,
  GridRowEditStopReasons,
} from '@mui/x-data-grid';
import { useEditContext } from 'react-admin';
import { useController } from 'react-hook-form';

interface Props {
  source: string;
}

const HeadersDataGrid = ({ source }: Props) => {
  const { record } = useEditContext();
  const [rows, setRows] = React.useState<GridRowsProp>(() => {
    const headers: [string, string][] = record[source];

    return headers.map(([name, value]) => {
      return {
        id: name,
        name,
        value,
      };
    });
  });

  const [rowModesModel, setRowModesModel] = React.useState<GridRowModesModel>({});

  const { field } = useController({
    name: source,
    defaultValue: JSON.stringify(Object.entries(rows)),
  });

  const handleRowEditStop: GridEventListener<'rowEditStop'> = (params, event) => {
    if (params.reason === GridRowEditStopReasons.rowFocusOut) {
      event.defaultMuiPrevented = true;
    }
  };

  const processRowUpdate = (newRow: GridRowModel) => {
    const updatedRow = { ...newRow };
    const updatedRows = rows.map((row) => (row.id === newRow.id ? updatedRow : row));
    setRows(updatedRows);

    const headers = updatedRows.map((row) => {
      return [row.name, row.value];
    });

    field.onChange(headers);

    return updatedRow;
  };

  const handleRowModesModelChange = (newRowModesModel: GridRowModesModel) => {
    setRowModesModel(newRowModesModel);
  };

  const columns: GridColDef[] = [
    { field: 'name', headerName: 'Name', width: 180, editable: true },
    { field: 'value', headerName: 'Value', width: 180, editable: true },
  ];

  // hideFooter prop is not working, so we use an empty footer slot
  return (
    <Box
      sx={{
        height: 500,
        width: '100%',
        '& .actions': {
          color: 'text.secondary',
        },
        '& .textPrimary': {
          color: 'text.primary',
        },
      }}
    >
      <DataGrid
        rows={rows}
        columns={columns}
        editMode="row"
        rowModesModel={rowModesModel}
        onRowModesModelChange={handleRowModesModelChange}
        onRowEditStop={handleRowEditStop}
        processRowUpdate={processRowUpdate}
        slots={{
          footer: () => <></>,
        }}
        slotProps={{}}
        hideFooter={false}
      />
    </Box>
  );
};

export default HeadersDataGrid;
