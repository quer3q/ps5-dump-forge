// The one "Delete this folder/image?" question: it only asks; the caller calls the api and owns the errors.

import { Modal, openOverlay } from "./common";
import { useMessages } from "./i18n";
import { Icon } from "./icons";

export type DeletedFile = "image" | "archive" | "file";

/** Asks before deleting `path` for good; No first and focused. `file`: an archive's other parts stay. */
export function confirmDelete(path: string, folder: boolean, file: DeletedFile = "image"): Promise<boolean> {
  return new Promise((resolve) => {
    let yes = false;
    openOverlay(
      (close) => (
        <DeleteDialog
          path={path}
          folder={folder}
          file={folder ? "image" : file}
          close={close}
          onYes={() => {
            yes = true;
            close();
          }}
        />
      ),
      () => resolve(yes),
    );
  });
}

function DeleteDialog(props: {
  path: string;
  folder: boolean;
  file: DeletedFile;
  close: () => void;
  onYes: () => void;
}) {
  const t = useMessages().delete;
  return (
    <Modal
      close={props.close}
      className="modal card"
      role="alertdialog"
      labelledBy="del-title"
      describedBy="del-text"
    >
      <div className="modal-head">
        <span className="head-icon warn">
          <Icon name="trash" />
        </span>
        <h2 id="del-title">
          {props.file === "archive" ? t.archiveTitle : props.file === "file" ? t.fileTitle : t.title(props.folder)}
        </h2>
      </div>
      <div id="del-text">
        <p className="out-box path">{props.path}</p>
        <p className="muted">
          {props.file === "archive" ? t.archiveText : props.file === "file" ? t.fileText : t.text(props.folder)}
        </p>
      </div>
      <div className="row end">
        <button autoFocus onClick={props.close}>
          {t.no}
        </button>
        <button className="danger" onClick={props.onYes}>
          {t.yes}
        </button>
      </div>
    </Modal>
  );
}
