#pragma once
#include <QDrag>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMimeData>
#include <QPointer>
#include <QStandardItemModel>
#include <QTreeView>
#include <QUuid>
#include <functional>

// The shared session owns ordering/undo. Qt supplies the native drop indicator and
// hit testing, but must never remove model rows after a successful external move.
class LayerTreeView : public QTreeView {
  public:
    using QTreeView::QTreeView;

  protected:
    void startDrag(Qt::DropActions) override {
        QPointer<QDrag> drag = new QDrag(this);
        drag->setMimeData(model()->mimeData(selectionModel()->selectedRows()));
        drag->exec(Qt::MoveAction | Qt::CopyAction, Qt::MoveAction);
        if (drag) drag->deleteLater();
    }
};
class LayerDragModel : public QStandardItemModel {
  public:
    using QStandardItemModel::QStandardItemModel;
    std::function<bool(QJsonObject, bool)> place;
    QString token = QUuid::createUuid().toString();
    QStringList mimeTypes() const override { return {"application/x-compositor-layers"}; }
    Qt::DropActions supportedDropActions() const override { return Qt::MoveAction | Qt::CopyAction; }
    QMimeData *mimeData(const QModelIndexList &indexes) const override {
        QJsonArray ids;
        for (const auto &index : indexes)
            if (index.column() == 0)
                ids.append(index.data(Qt::UserRole).toString());
        auto *data = new QMimeData;
        data->setData(mimeTypes().first(),
                      QJsonDocument(QJsonObject{{"source", token}, {"layerIDs", ids}}).toJson());
        return data;
    }
    bool request(const QMimeData *data, Qt::DropAction action, int row, int column, const QModelIndex &parent,
                 bool check) const {
        if (!place || column > 0 || (action != Qt::MoveAction && action != Qt::CopyAction))
            return false;
        auto command = QJsonDocument::fromJson(data->data(mimeTypes().first())).object();
        if (command.value("source").toString() != token || command.value("layerIDs").toArray().isEmpty())
            return false;
        if (parent.isValid() && !parent.data(Qt::UserRole + 1).toBool())
            return false;
        command.insert("action", check ? "validateLayerDrop" : "placeLayers");
        command.insert("parentID", parent.isValid() ? QJsonValue(parent.data(Qt::UserRole).toString())
                                                    : QJsonValue(QJsonValue::Null));
        // A row of -1 means on a folder (top), or empty viewport (root bottom).
        const auto target = row >= 0 ? index(row, 0, parent) : QModelIndex();
        if (target.isValid())
            command.insert("targetID", target.data(Qt::UserRole).toString());
        command.insert("forward", !target.isValid() && (row >= 0 || !parent.isValid()));
        command.insert("enabled", action == Qt::CopyAction);
        return place(command, check);
    }
    bool canDropMimeData(const QMimeData *d, Qt::DropAction a, int r, int c,
                         const QModelIndex &p) const override {
        return request(d, a, r, c, p, true);
    }
    bool dropMimeData(const QMimeData *d, Qt::DropAction a, int r, int c, const QModelIndex &p) override {
        return request(d, a, r, c, p, false);
    }
};
