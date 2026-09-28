#ifndef LAYER_ITEM_DELEGATE_H
#define LAYER_ITEM_DELEGATE_H

// Layer-panel row: eye toggle, checkerboard thumbnail, name and a
// "W × H px" / "Folder" subtitle. Header-only and Q_OBJECT-free so the
// SwiftPM HostRun target (which cannot run moc) needs no extra sources.

#include <QAbstractItemView>
#include <QApplication>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QStandardItemModel>
#include <QStyledItemDelegate>

class LayerItemDelegate : public QStyledItemDelegate {
public:
    enum Role { ThumbnailRole = Qt::UserRole + 5, SubtitleRole = Qt::UserRole + 6, AdjustmentRole = Qt::UserRole + 7 };
    static constexpr int RowHeight = 52;
    static constexpr int EyeLeft = 12;

    using QStyledItemDelegate::QStyledItemDelegate;

    QSize sizeHint(const QStyleOptionViewItem &option, const QModelIndex &) const override {
        return QSize(option.rect.width(), RowHeight);
    }

    void paint(QPainter *p, const QStyleOptionViewItem &option, const QModelIndex &index) const override {
        p->save();
        p->setRenderHint(QPainter::Antialiasing, true);
        const int viewWidth = option.widget ? option.widget->width() : option.rect.right() + 1;
        const QRect row(0, option.rect.top(), viewWidth, option.rect.height());
        const bool selected = option.state & QStyle::State_Selected;
        const bool visible = index.siblingAtColumn(1).data(Qt::CheckStateRole).toInt() == Qt::Checked;

        if (selected) p->fillRect(row, QColor(0x3a, 0x3b, 0x3f));
        else if (option.state & QStyle::State_MouseOver) p->fillRect(row, QColor(0x2c, 0x2d, 0x30));
        p->setPen(QColor(0x2a, 0x2b, 0x2e));
        p->drawLine(row.bottomLeft(), row.bottomRight());

        drawEye(p, eyeRect(option.rect), visible, selected);

        const int thumbSide = 36;
        const QRect thumb(option.rect.left() + 40, option.rect.center().y() - thumbSide / 2, thumbSide, thumbSide);
        drawThumbnail(p, thumb, index, selected);

        const int textLeft = thumb.right() + 12;
        const QRect textArea(textLeft, option.rect.top() + 6, viewWidth - textLeft - 10, option.rect.height() - 12);
        QFont nameFont = option.font;
        nameFont.setPointSizeF(qMax(9.5, nameFont.pointSizeF() * 1.05));
        p->setFont(nameFont);
        p->setPen(visible ? QColor(0xf2, 0xf2, 0xf5) : QColor(0x8a, 0x8a, 0x90));
        const QString name = index.data(Qt::DisplayRole).toString();
        p->drawText(QRect(textArea.left(), textArea.top(), textArea.width(), textArea.height() / 2 + 2),
                    Qt::AlignLeft | Qt::AlignBottom,
                    QFontMetrics(nameFont).elidedText(name, Qt::ElideRight, textArea.width()));
        QFont subFont = option.font;
        subFont.setPointSizeF(qMax(8.0, subFont.pointSizeF() * 0.85));
        p->setFont(subFont);
        p->setPen(QColor(0x8e, 0x8e, 0x93));
        QString subtitle = index.data(SubtitleRole).toString();
        if (index.siblingAtColumn(2).data(Qt::CheckStateRole).isValid()) {
            const bool maskOn = index.siblingAtColumn(2).data(Qt::CheckStateRole).toInt() == Qt::Checked;
            subtitle += maskOn ? QStringLiteral("  ·  mask") : QStringLiteral("  ·  mask off");
        }
        p->drawText(QRect(textArea.left(), textArea.center().y() + 3, textArea.width(), textArea.height() / 2),
                    Qt::AlignLeft | Qt::AlignTop,
                    QFontMetrics(subFont).elidedText(subtitle, Qt::ElideRight, textArea.width()));
        p->restore();
    }

    bool editorEvent(QEvent *event, QAbstractItemModel *model, const QStyleOptionViewItem &option,
                     const QModelIndex &index) override {
        if (event->type() == QEvent::MouseButtonRelease) {
            auto *mouse = static_cast<QMouseEvent *>(event);
            if (mouse->button() == Qt::LeftButton && eyeRect(option.rect).adjusted(-6, -6, 6, 6).contains(mouse->pos())) {
                const QModelIndex visIndex = index.siblingAtColumn(1);
                if (visIndex.flags() & Qt::ItemIsUserCheckable) {
                    const bool on = visIndex.data(Qt::CheckStateRole).toInt() == Qt::Checked;
                    model->setData(visIndex, on ? Qt::Unchecked : Qt::Checked, Qt::CheckStateRole);
                }
                return true;
            }
        }
        return QStyledItemDelegate::editorEvent(event, model, option, index);
    }

    void updateEditorGeometry(QWidget *editor, const QStyleOptionViewItem &option, const QModelIndex &) const override {
        editor->setGeometry(option.rect.left() + 88, option.rect.top() + 8, option.rect.width() - 96, 22);
    }

private:
    static QRect eyeRect(const QRect &rowRect) {
        return QRect(EyeLeft, rowRect.center().y() - 8, 18, 16);
    }

    static void drawEye(QPainter *p, const QRect &r, bool visible, bool selected) {
        const QColor color = visible ? (selected ? QColor(0xff, 0xff, 0xff) : QColor(0xd8, 0xd8, 0xdc))
                                     : QColor(0x6a, 0x6a, 0x70);
        p->setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap));
        p->setBrush(Qt::NoBrush);
        QPainterPath lid;
        const QPointF c = r.center();
        lid.moveTo(r.left(), c.y());
        lid.quadTo(c.x(), r.top() - 3, r.right(), c.y());
        lid.quadTo(c.x(), r.bottom() + 3, r.left(), c.y());
        p->drawPath(lid);
        if (visible) {
            p->setBrush(color);
            p->drawEllipse(c, 2.6, 2.6);
        } else {
            p->drawLine(QPointF(r.left() + 2, r.bottom() + 1), QPointF(r.right() - 2, r.top() - 1));
        }
    }

    static void drawThumbnail(QPainter *p, const QRect &rect, const QModelIndex &index, bool selected) {
        const QPixmap pix = index.data(ThumbnailRole).value<QPixmap>();
        const bool isGroup = index.data(Qt::UserRole + 1).toBool();
        if (isGroup || index.data(AdjustmentRole).toBool()) {
            p->setPen(Qt::NoPen);
            p->setBrush(QColor(0x2f, 0x30, 0x34));
            p->drawRoundedRect(rect, 4, 4);
            p->setPen(QPen(QColor(0xc8, 0xc8, 0xce), 1.4));
            p->setBrush(Qt::NoBrush);
            if (isGroup) {
                p->drawRoundedRect(QRectF(rect.left() + 9, rect.top() + 12, 18, 13), 2.5, 2.5);
                p->drawRect(QRectF(rect.left() + 9, rect.top() + 9, 8, 4));
            } else {
                p->drawEllipse(QRectF(rect.left() + 9, rect.top() + 9, 18, 18));
                p->setBrush(QColor(0xc8, 0xc8, 0xce));
                QPainterPath half;
                half.moveTo(rect.center().x(), rect.top() + 9);
                half.arcTo(QRectF(rect.left() + 9, rect.top() + 9, 18, 18), 90, 180);
                half.closeSubpath();
                p->drawPath(half);
            }
            return;
        }
        p->setClipRect(rect);
        static QPixmap checker;
        if (checker.isNull()) {
            QImage chk(8, 8, QImage::Format_RGB32);
            chk.fill(QColor(0x9a, 0x9a, 0x9e));
            QPainter cp(&chk);
            cp.fillRect(0, 0, 4, 4, QColor(0x6c, 0x6c, 0x70));
            cp.fillRect(4, 4, 4, 4, QColor(0x6c, 0x6c, 0x70));
            checker = QPixmap::fromImage(chk);
        }
        p->drawTiledPixmap(rect, checker);
        if (!pix.isNull()) {
            const QSizeF fit = QSizeF(pix.size()).scaled(rect.size(), Qt::KeepAspectRatio);
            const QRectF target(rect.center().x() - fit.width() / 2, rect.center().y() - fit.height() / 2,
                                fit.width(), fit.height());
            p->drawPixmap(target, pix, QRectF(pix.rect()));
        }
        p->setClipping(false);
        p->setPen(QPen(selected ? QColor(0xf2, 0xf2, 0xf5) : QColor(0x55, 0x56, 0x5a), selected ? 1.6 : 1.0));
        p->setBrush(Qt::NoBrush);
        p->drawRoundedRect(QRectF(rect).adjusted(0.5, 0.5, -0.5, -0.5), 3, 3);
    }
};

#endif
