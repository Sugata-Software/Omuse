#pragma once
#include <QDialog>
#include <QDialogButtonBox>
#include <QHeaderView>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QPushButton>
#include <QTableWidget>
#include <QVBoxLayout>
#include <functional>

namespace shortcuts {
inline QKeySequence sequence(const QString &key, int bits) {
    const QMap<QString, int> special{{"\x7f", Qt::Key_Delete},
                                     {"\r", Qt::Key_Return},
                                     {"\x1b", Qt::Key_Escape},
                                     {"\t", Qt::Key_Tab},
                                     {" ", Qt::Key_Space},
                                     {QString(QChar(0xf702)), Qt::Key_Left},
                                     {QString(QChar(0xf703)), Qt::Key_Right},
                                     {QString(QChar(0xf700)), Qt::Key_Up},
                                     {QString(QChar(0xf701)), Qt::Key_Down}};
    int code = special.value(key, key.isEmpty() ? 0 : key.toUpper().at(0).unicode());
    return QKeySequence(code | (bits & 1 ? Qt::CTRL : 0) | (bits & 2 ? Qt::ALT : 0) |
                        (bits & 4 ? Qt::META : 0) | (bits & 8 ? Qt::SHIFT : 0));
}
inline QJsonObject chord(const QKeySequence &seq) {
    if (seq.isEmpty())
        return {{"key", ""}, {"modifiers", 0}};
    auto combo = seq[0];
    int code = combo.key();
    auto mods = combo.keyboardModifiers();
    const QMap<int, QString> special{{Qt::Key_Delete, "\x7f"},
                                     {Qt::Key_Backspace, "\x7f"},
                                     {Qt::Key_Return, "\r"},
                                     {Qt::Key_Enter, "\r"},
                                     {Qt::Key_Escape, "\x1b"},
                                     {Qt::Key_Tab, "\t"},
                                     {Qt::Key_Space, " "},
                                     {Qt::Key_Left, QString(QChar(0xf702))},
                                     {Qt::Key_Right, QString(QChar(0xf703))},
                                     {Qt::Key_Up, QString(QChar(0xf700))},
                                     {Qt::Key_Down, QString(QChar(0xf701))}};
    QString key = special.value(code, code < 0x10000 ? QString(QChar(code)).toLower() : QString());
    const QMap<QString, QString> shifted{{"{", "["}, {"}", "]"}, {"+", "="}, {"_", "-"}};
    key = shifted.value(key, key);
    return {{"key", key},
            {"modifiers", (mods & Qt::ControlModifier ? 1 : 0) | (mods & Qt::AltModifier ? 2 : 0) |
                              (mods & Qt::MetaModifier ? 4 : 0) | (mods & Qt::ShiftModifier ? 8 : 0)}};
}
inline const QMap<QString, QString> menus{{"Undo", "edit.undo"},
                                          {"Redo", "edit.redo"},
                                          {"New Canvas", "file.new"},
                                          {"Open Project", "file.openProject"},
                                          {"Save", "file.save"},
                                          {"Save As", "file.saveAs"},
                                          {"Export PNG", "file.exportPNG"},
                                          {"Export JPEG", "file.exportJPEG"},
                                          {"Close Project", "file.closeProject"},
                                          {"Fit Canvas", "view.fitCanvas"},
                                          {"Actual Pixels", "view.actualPixels"},
                                          {"Zoom In", "view.zoomIn"},
                                          {"Zoom Out", "view.zoomOut"},
                                          {"Show Transform Controls", "view.transformControls"},
                                          {"Cut", "edit.cut"},
                                          {"Copy", "edit.copy"},
                                          {"Copy Merged", "edit.copyMerged"},
                                          {"Paste", "edit.paste"},
                                          {"Fill with Foreground", "fill.foreground"},
                                          {"Fill with Background", "fill.background"},
                                          {"Content-Aware Fill", "edit.contentFill"},
                                          {"Select All", "select.all"},
                                          {"Deselect", "select.deselect"},
                                          {"Inverse Selection", "select.inverse"},
                                          {"Select Subject", "select.subject"},
                                          {"Curves", "adjust.Curves"},
                                          {"Levels", "adjust.Levels"},
                                          {"Hue/Saturation", "adjust.Hue/Saturation"},
                                          {"Invert Pixels / Mask", "image.invert"},
                                          {"Canvas Size", "canvasSize"},
                                          {"Image Size", "imageSize"},
                                          {"Transform Layer / Selection", "layer.transform"},
                                          {"Duplicate / Layer via Copy", "layer.duplicate"},
                                          {"Toggle Clipping Mask", "layer.clippingMask"},
                                          {"Group Layers", "layer.addGroup"},
                                          {"New Blank Layer", "layer.new"},
                                          {"Move Layer Up", "layer.moveUp"},
                                          {"Move Layer Down", "layer.moveDown"},
                                          {"Merge Layers", "layer.merge"},
                                          {"Show Grid", "view.showGrid"},
                                          {"Show Guides", "view.showGuides"},
                                          {"Show Rulers", "view.rulers"},
                                          {"Snap", "view.snap"},
                                          {"Lock Guides", "view.lockGuides"}};
inline bool supported(const QJsonObject &d) {
    if (d.value("group") == "Menus")
        return menus.contains(d.value("title").toString());
    if (d.value("group") != "Canvas & Layers")
        return false;
    const auto key = d.value("originalKey").toString();
    const int mods = d.value("originalModifiers").toInt();
    return (mods == 0 && QString("vmlwcb ejsr gutihzxd\t \x1b\r[]").contains(key) && key.size() == 1) ||
           (mods == 8 && (key == "[" || key == "]" || key == "u"));
}
inline void edit(QWidget *parent, const QJsonArray &definitions, std::function<QString(QJsonObject)> save) {
    QDialog dialog(parent);
    dialog.setObjectName("shortcuts.dialog");
    dialog.setWindowTitle("Keyboard Shortcuts");
    dialog.resize(660, 560);
    auto *layout = new QVBoxLayout(&dialog);
    layout->addWidget(new QLabel(
        "Click a shortcut and press a key combination. Ctrl is the Linux command modifier.", &dialog));
    auto *table = new QTableWidget(0, 2, &dialog);
    table->setObjectName("shortcuts.table");
    table->setHorizontalHeaderLabels({"Action", "Shortcut"});
    table->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    layout->addWidget(table);
    QList<QPair<QJsonObject, QKeySequenceEdit *>> fields;
    for (const auto &v : definitions) {
        auto d = v.toObject();
        if (!supported(d))
            continue;
        int row = table->rowCount();
        table->insertRow(row);
        auto *item = new QTableWidgetItem(d.value("title").toString());
        item->setFlags(Qt::ItemIsEnabled);
        table->setItem(row, 0, item);
        auto *field =
            new QKeySequenceEdit(sequence(d.value("key").toString(), d.value("modifiers").toInt()), table);
        field->setMaximumSequenceLength(1);
        field->setClearButtonEnabled(true);
        field->setObjectName(d.value("id").toString());
        field->setAccessibleName(d.value("title").toString());
        table->setCellWidget(row, 1, field);
        fields.append({d, field});
    }
    auto *error = new QLabel(&dialog);
    error->setObjectName("shortcuts.error");
    error->setWordWrap(true);
    layout->addWidget(error);
    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Save | QDialogButtonBox::Cancel | QDialogButtonBox::RestoreDefaults, &dialog);
    layout->addWidget(buttons);
    QObject::connect(buttons->button(QDialogButtonBox::RestoreDefaults), &QPushButton::clicked, &dialog, [&] {
        for (auto &[d, field] : fields)
            field->setKeySequence(
                sequence(d.value("originalKey").toString(), d.value("originalModifiers").toInt()));
    });
    QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, [&] {
        QJsonObject values;
        for (auto v : definitions) {
            auto d = v.toObject();
            values.insert(d.value("id").toString(),
                          QJsonObject{{"key", d.value("key")}, {"modifiers", d.value("modifiers")}});
        }
        for (auto &[d, field] : fields)
            values.insert(d.value("id").toString(), chord(field->keySequence()));
        auto problem = save(values);
        error->setText(problem);
        if (problem.isEmpty())
            dialog.accept();
    });
    dialog.exec();
}
} // namespace shortcuts
