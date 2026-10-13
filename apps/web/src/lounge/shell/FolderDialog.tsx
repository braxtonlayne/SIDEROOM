import { Button, Modal, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { useRail } from '../data';
import { useLoungeUi } from '../store';
import { deleteFolder, FOLDER_COLORS, FOLDER_NAME_MAX, updateFolder } from './railLayout';

/** Folder settings (LNG-RAIL-008): name and color; the folder can also be dissolved. */
export function FolderDialog({ folderId, onClose }: { folderId: string; onClose: () => void }) {
  const rail = useRail();
  const setLayout = useLoungeUi((s) => s.setRailLayout);
  const folder = rail.find((e) => e.kind === 'folder' && e.id === folderId);
  const [name, setName] = useState(folder?.kind === 'folder' ? (folder.name ?? '') : '');
  const [color, setColor] = useState(folder?.kind === 'folder' ? folder.color : FOLDER_COLORS[0]);
  if (!folder || folder.kind !== 'folder') return null;

  const save = (e: FormEvent) => {
    e.preventDefault();
    setLayout(updateFolder(rail, folderId, { name, color }));
    onClose();
  };

  return (
    <Modal open onClose={onClose} size="sm" title="Folder Settings">
      <form className="lng-form" onSubmit={save}>
        <TextInput
          label="Folder name"
          value={name}
          maxLength={FOLDER_NAME_MAX}
          autoFocus
          onChange={(e) => setName(e.target.value)}
        />
        <fieldset className="lng-swatches">
          <legend className="lng-field-label">Folder color</legend>
          <div className="lng-swatch-row">
            {FOLDER_COLORS.map((c) => (
              <label key={c} className="lng-swatch" style={{ background: c }}>
                <input
                  type="radio"
                  name="folder-color"
                  value={c}
                  checked={color === c}
                  aria-label={c}
                  onChange={() => setColor(c)}
                />
              </label>
            ))}
          </div>
        </fieldset>
        <div className="lng-form-actions">
          <Button
            variant="danger"
            onClick={() => {
              setLayout(deleteFolder(rail, folderId));
              onClose();
            }}
          >
            Ungroup folder
          </Button>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit">Save</Button>
        </div>
      </form>
    </Modal>
  );
}
